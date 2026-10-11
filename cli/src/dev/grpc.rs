//! Byte-level gRPC client (unary + server-streaming) for guest `net.grpc`.

use crate::hosts::{check_dev_target, dial_tcp, resolve_pinned, HostSpec};
use super::mailbox::Mail;
use bytes::{Buf, BufMut};
use http::uri::PathAndQuery;
use http::Uri;
use hyper_util::rt::TokioIo;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::sync::oneshot;
use tonic::client::Grpc;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Code, Request, Status};
use tower::Service;

const MAX_GRPC: u32 = 2;
const MAX_MSG: usize = 1024 * 1024;
const GRPC_TIMEOUT: Duration = Duration::from_secs(30);

pub struct GrpcResponse {
    pub status: u32,
    pub message: String,
    pub body: Vec<u8>,
}

struct GrpcSlot {
    cancel: Option<oneshot::Sender<()>>,
}

pub(crate) struct GrpcPluginState {
    next_id: AtomicU32,
    streams: std::sync::Arc<Mutex<HashMap<u32, GrpcSlot>>>,
}

impl GrpcPluginState {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            streams: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BytesCodec;

#[derive(Debug, Clone, Default)]
struct BytesEncoder;

#[derive(Debug, Clone, Default)]
struct BytesDecoder;

impl Codec for BytesCodec {
    type Encode = Vec<u8>;
    type Decode = Vec<u8>;
    type Encoder = BytesEncoder;
    type Decoder = BytesDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        BytesEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        BytesDecoder
    }
}

impl Encoder for BytesEncoder {
    type Item = Vec<u8>;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        if item.len() > MAX_MSG {
            return Err(Status::out_of_range("request too large"));
        }
        dst.put_slice(&item);
        Ok(())
    }
}

impl Decoder for BytesDecoder {
    type Item = Vec<u8>;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let len = src.remaining();
        if len == 0 {
            return Ok(None);
        }
        if len > MAX_MSG {
            return Err(Status::out_of_range("response too large"));
        }
        let mut out = vec![0u8; len];
        src.copy_to_slice(&mut out);
        Ok(Some(out))
    }
}

fn validate_path(path: &str) -> Result<PathAndQuery, String> {
    let path = path.trim();
    if !path.starts_with('/') || path.len() < 2 || path.contains(' ') {
        return Err("bad grpc path".into());
    }
    PathAndQuery::try_from(path).map_err(|err| format!("bad grpc path: {err}"))
}

fn validate_request(request: &[u8]) -> Result<(), String> {
    if request.len() > MAX_MSG {
        return Err("request too large".into());
    }
    Ok(())
}

fn check_grpc_authority(authority: &str, specs: &[HostSpec]) -> Result<(String, u16), String> {
    let authority = authority.trim().to_lowercase();
    if authority.is_empty() {
        return Err("empty authority".into());
    }
    let (host, port) = if let Some((h, p)) = authority.rsplit_once(':') {
        if p.chars().all(|c| c.is_ascii_digit()) {
            let port: u16 = p.parse().map_err(|_| "invalid port".to_string())?;
            (h.to_string(), port)
        } else {
            (authority.clone(), 443)
        }
    } else {
        (authority, 443)
    };
    let url = if port == 443 {
        format!("https://{host}/")
    } else {
        format!("https://{host}:{port}/")
    };
    check_dev_target(&url, specs, false)?;
    Ok((host, port))
}

#[derive(Clone)]
struct PinnedConnector {
    addrs: Vec<SocketAddr>,
}

impl Service<Uri> for PinnedConnector {
    type Response = TokioIo<TokioTcpStream>;
    type Error = std::io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _dst: Uri) -> Self::Future {
        let addrs = self.addrs.clone();
        Box::pin(async move {
            let std_stream = dial_tcp(&addrs, GRPC_TIMEOUT)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;
            std_stream.set_nonblocking(true)?;
            let stream = TokioTcpStream::from_std(std_stream)?;
            Ok(TokioIo::new(stream))
        })
    }
}

async fn connect_channel(host: &str, port: u16, addrs: Vec<SocketAddr>) -> Result<Channel, String> {
    let uri = if port == 443 {
        format!("https://{host}")
    } else {
        format!("https://{host}:{port}")
    };
    let tls = ClientTlsConfig::new().with_native_roots();
    Endpoint::from_shared(uri)
        .map_err(|err| err.to_string())?
        .tls_config(tls)
        .map_err(|err| err.to_string())?
        .connect_timeout(GRPC_TIMEOUT)
        .timeout(GRPC_TIMEOUT)
        .connect_with_connector(PinnedConnector { addrs })
        .await
        .map_err(|err| err.to_string())
}

pub(crate) fn validate_guest_metadata(headers: &[(String, String)]) -> Result<(), String> {
    for (k, _) in headers {
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let lower = key.to_ascii_lowercase();
        if key.starts_with(':') || lower.starts_with("grpc-") {
            return Err(format!("forbidden header {key}"));
        }
    }
    Ok(())
}

fn apply_metadata(
    mut req: Request<Vec<u8>>,
    headers: &[(String, String)],
) -> Result<Request<Vec<u8>>, String> {
    validate_guest_metadata(headers)?;
    let meta = req.metadata_mut();
    for (k, v) in headers {
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let key = tonic::metadata::MetadataKey::from_bytes(key.as_bytes())
            .map_err(|_| format!("bad header key {key}"))?;
        let value = tonic::metadata::MetadataValue::try_from(v.as_str())
            .map_err(|_| format!("bad header value for {key}"))?;
        meta.insert(key, value);
    }
    Ok(req)
}

fn status_to_u32(code: Code) -> u32 {
    code as i32 as u32
}

pub fn grpc_call(
    specs: &[HostSpec],
    authority: &str,
    path: &str,
    headers: &[(String, String)],
    request: &[u8],
) -> Result<GrpcResponse, String> {
    validate_guest_metadata(headers)?;
    let (host, port) = check_grpc_authority(authority, specs)?;
    let addrs = resolve_pinned(&host, port)?;
    let path = validate_path(path)?;
    validate_request(request)?;
    let headers = headers.to_vec();
    let body = request.to_vec();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;

    rt.block_on(async move {
        let channel = connect_channel(&host, port, addrs).await?;
        let mut client = Grpc::new(channel);
        client
            .ready()
            .await
            .map_err(|err| format!("grpc not ready: {err}"))?;
        let req = apply_metadata(Request::new(body), &headers)?;
        match client.unary(req, path, BytesCodec).await {
            Ok(resp) => Ok(GrpcResponse {
                status: 0,
                message: String::new(),
                body: resp.into_inner(),
            }),
            Err(status) => Ok(GrpcResponse {
                status: status_to_u32(status.code()),
                message: status.message().to_string(),
                body: Vec::new(),
            }),
        }
    })
}

pub fn grpc_open_stream(
    state: &GrpcPluginState,
    specs: &[HostSpec],
    authority: &str,
    path: &str,
    headers: &[(String, String)],
    request: &[u8],
    mailbox: SyncSender<Mail>,
) -> Result<u32, String> {
    validate_guest_metadata(headers)?;
    let (host, port) = check_grpc_authority(authority, specs)?;
    let addrs = resolve_pinned(&host, port)?;
    let path = validate_path(path)?;
    validate_request(request)?;
    {
        let streams = state.streams.lock().map_err(|err| err.to_string())?;
        if streams.len() as u32 >= MAX_GRPC {
            return Err("grpc concurrent streams".into());
        }
    }

    let id = state.next_id.fetch_add(1, Ordering::SeqCst);
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    {
        let mut streams = state.streams.lock().map_err(|err| err.to_string())?;
        streams.insert(id, GrpcSlot {
            cancel: Some(cancel_tx),
        });
    }

    let headers = headers.to_vec();
    let body = request.to_vec();
    let streams = std::sync::Arc::clone(&state.streams);
    let thread_name = format!("grpc-{id}");
    let spawn = thread::Builder::new().name(thread_name).spawn(move || {
        let finish = |status: u32, message: String| {
            if let Ok(mut map) = streams.lock() { map.remove(&id); }
            let _ = mailbox.try_send(Mail::GrpcClosed {
                handle: id,
                status,
                message,
            });
        };
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                finish(status_to_u32(Code::Internal), err.to_string());
                return;
            }
        };
        rt.block_on(stream_worker(
            id,
            host,
            port,
            addrs,
            path,
            headers,
            body,
            cancel_rx,
            mailbox,
            streams,
        ));
    });
    if let Err(err) = spawn {
        if let Ok(mut map) = state.streams.lock() { map.remove(&id); }
        return Err(err.to_string());
    }

    Ok(id)
}

async fn stream_worker(
    handle: u32,
    host: String,
    port: u16,
    addrs: Vec<SocketAddr>,
    path: PathAndQuery,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    cancel_rx: oneshot::Receiver<()>,
    mailbox: SyncSender<Mail>,
    streams: std::sync::Arc<Mutex<HashMap<u32, GrpcSlot>>>,
) {
    let close = |status: u32, message: String| {
        if let Ok(mut map) = streams.lock() { map.remove(&handle); }
        let _ = mailbox.try_send(Mail::GrpcClosed {
            handle,
            status,
            message,
        });
    };

    let channel = match connect_channel(&host, port, addrs).await {
        Ok(c) => c,
        Err(err) => {
            close(status_to_u32(Code::Unavailable), err);
            return;
        }
    };
    let mut client = Grpc::new(channel);
    if let Err(err) = client.ready().await {
        close(
            status_to_u32(Code::Unavailable),
            format!("grpc not ready: {err}"),
        );
        return;
    }
    let req = match apply_metadata(Request::new(body), &headers) {
        Ok(r) => r,
        Err(err) => {
            close(status_to_u32(Code::InvalidArgument), err);
            return;
        }
    };

    let result = client.server_streaming(req, path, BytesCodec).await;
    let mut stream = match result {
        Ok(resp) => resp.into_inner(),
        Err(status) => {
            close(status_to_u32(status.code()), status.message().to_string());
            return;
        }
    };

    tokio::pin!(cancel_rx);
    loop {
        tokio::select! {
            _ = &mut cancel_rx => {
                close(status_to_u32(Code::Cancelled), "cancelled".into());
                return;
            }
            next = stream.message() => {
                match next {
                    Ok(Some(bytes)) => {
                        if mailbox.try_send(Mail::GrpcMsg { handle, bytes }).is_err() {
                            if let Ok(mut map) = streams.lock() { map.remove(&handle); }
                            return;
                        }
                    }
                    Ok(None) => {
                        close(0, String::new());
                        return;
                    }
                    Err(status) => {
                        close(status_to_u32(status.code()), status.message().to_string());
                        return;
                    }
                }
            }
        }
    }
}

pub fn grpc_cancel(state: &GrpcPluginState, handle: u32) -> Result<(), String> {
    let mut streams = state.streams.lock().map_err(|err| err.to_string())?;
    if let Some(mut slot) = streams.remove(&handle) {
        if let Some(tx) = slot.cancel.take() {
            let _ = tx.send(());
        }
    }
    Ok(())
}

pub fn grpc_close_all(state: &GrpcPluginState) -> Result<(), String> {
    let mut streams = state.streams.lock().map_err(|err| err.to_string())?;
    for (_, mut slot) in streams.drain() {
        if let Some(tx) = slot.cancel.take() {
            let _ = tx.send(());
        }
    }
    Ok(())
}


