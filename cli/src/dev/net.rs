use super::mailbox::{Mail, Mailbox};
use crate::hosts::{
    check_dev_target, dial_tcp, https_url_host, resolve_pinned, url_without_query, HostSpec,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use url::Url;

const FORBIDDEN_GUEST_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "keep-alive",
    "te",
    "trailer",
    "upgrade",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
];

/// One hop without auto-follow. Used by production `reqwest` and unit tests.
pub(crate) struct HopResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub location: Option<String>,
}

fn guest_header_forbidden(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    FORBIDDEN_GUEST_HEADERS.contains(&lower.as_str())
}

pub(crate) fn reject_guest_headers(headers: &[(String, String)]) -> Result<(), String> {
    for (k, _) in headers {
        if guest_header_forbidden(k) {
            return Err(format!("forbidden header {}", k.trim()));
        }
    }
    Ok(())
}

fn url_origin(url: &Url) -> Option<(String, String, u16)> {
    let host = url.host_str()?.to_ascii_lowercase();
    let port = url.port_or_known_default()?;
    Some((url.scheme().to_string(), host, port))
}

fn same_origin(a: &Url, b: &Url) -> bool {
    match (url_origin(a), url_origin(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn redirect_method_and_body(
    status: u16,
    method: reqwest::Method,
    body: Vec<u8>,
) -> (reqwest::Method, Vec<u8>) {
    match status {
        301 | 302 | 303 if method != reqwest::Method::GET && method != reqwest::Method::HEAD => {
            (reqwest::Method::GET, Vec::new())
        }
        _ => (method, body),
    }
}

fn resolve_redirect_url(current: &Url, location: &str) -> Result<Url, String> {
    current
        .join(location)
        .map_err(|err| format!("redirect location: {err}"))
}

/// Manual redirect walk: check every hop before send; strip guest headers on cross-origin.
pub(crate) fn fetch_with_redirects(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
    mut check: impl FnMut(&str) -> Result<(), String>,
    mut send: impl FnMut(
        &str,
        &reqwest::Method,
        &[(String, String)],
        &[u8],
    ) -> Result<HopResponse, String>,
) -> Result<HttpResponse, String> {
    reject_guest_headers(headers)?;
    let mut method: reqwest::Method = method
        .parse()
        .map_err(|_| "invalid method".to_string())?;
    let mut url = Url::parse(url).map_err(|err| format!("url: {err}"))?;
    let mut headers = headers.to_vec();
    let mut body = body.to_vec();
    let mut redirects: usize = 0;

    loop {
        check(url.as_str())?;
        let hop = send(url.as_str(), &method, &headers, &body)?;
        if !is_redirect_status(hop.status) {
            if hop.body.len() > MAX_BODY {
                return Err("response too large".into());
            }
            return Ok(HttpResponse {
                status: hop.status,
                headers: hop.headers,
                body: hop.body,
            });
        }
        let Some(location) = hop.location.as_deref().filter(|s| !s.is_empty()) else {
            if hop.body.len() > MAX_BODY {
                return Err("response too large".into());
            }
            return Ok(HttpResponse {
                status: hop.status,
                headers: hop.headers,
                body: hop.body,
            });
        };
        redirects += 1;
        if redirects > MAX_REDIRECTS {
            return Err("redirect chain too long".into());
        }
        let next = resolve_redirect_url(&url, location)?;
        if !same_origin(&url, &next) {
            headers.clear();
        }
        let (next_method, next_body) = redirect_method_and_body(hop.status, method, body);
        method = next_method;
        body = next_body;
        url = next;
    }
}

struct PinnedResolver;

impl reqwest::dns::Resolve for PinnedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        Box::pin(async move {
            let addrs = resolve_pinned(&host, 0).map_err(|err| {
                Box::new(std::io::Error::new(ErrorKind::Other, err))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
            let iter: Box<dyn Iterator<Item = SocketAddr> + Send> = Box::new(addrs.into_iter());
            Ok(iter)
        })
    }
}

const MAX_REDIRECTS: usize = 5;
const MAX_BODY: usize = 4 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const WS_WRITE_TIMEOUT: Duration = Duration::from_secs(15);
const WS_READ_POLL: Duration = Duration::from_millis(50);
const MAX_HTTP: u32 = 4;
const MAX_WS: u32 = 2;

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub struct HttpFixture {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

enum WsCmd {
    Send {
        text: String,
        reply: Sender<Result<(), String>>,
    },
    Close,
}

enum WsSlot {
    Replay {
        closed: Arc<AtomicBool>,
    },
    Live {
        cmds: Sender<WsCmd>,
        shutdown: TcpStream,
    },
}

pub struct DevNet {
    specs: Vec<HostSpec>,
    mailbox: Arc<Mailbox>,
    stop: Arc<AtomicBool>,
    replay: Option<Arc<[String]>>,
    http_fixtures: HashMap<String, HttpFixture>,
    client: reqwest::blocking::Client,
    inflight_http: AtomicU32,
    next_ws: AtomicU32,
    sockets: Mutex<HashMap<u32, WsSlot>>,
    grpc: super::grpc::GrpcPluginState,
}

impl DevNet {
    pub fn new(
        specs: Vec<HostSpec>,
        mailbox: Arc<Mailbox>,
        stop: Arc<AtomicBool>,
        replay: Option<Vec<String>>,
        http_fixtures: HashMap<String, HttpFixture>,
    ) -> Result<Arc<Self>, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_TIMEOUT)
            .dns_resolver(Arc::new(PinnedResolver))
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Arc::new(Self {
            specs,
            mailbox,
            stop,
            replay: replay.map(|frames| Arc::from(frames)),
            http_fixtures,
            client,
            inflight_http: AtomicU32::new(0),
            next_ws: AtomicU32::new(1),
            sockets: Mutex::new(HashMap::new()),
            grpc: super::grpc::GrpcPluginState::new(),
        }))
    }

    pub fn grpc_open_stream(
        &self,
        authority: &str,
        path: &str,
        headers: &[(String, String)],
        request: &[u8],
    ) -> Result<u32, String> {
        super::grpc::grpc_open_stream(
            &self.grpc,
            &self.specs,
            authority,
            path,
            headers,
            request,
            self.mailbox.sender(),
        )
    }

    pub fn grpc_cancel(&self, handle: u32) -> Result<(), String> {
        super::grpc::grpc_cancel(&self.grpc, handle)
    }

    pub fn fetch(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<HttpResponse, String> {
        if body.len() > MAX_BODY {
            return Err("body too large".into());
        }
        let key = url_without_query(url);
        let fixture = self.http_fixtures.get(&key);
        let current = self.inflight_http.fetch_add(1, Ordering::SeqCst);
        if current >= MAX_HTTP {
            self.inflight_http.fetch_sub(1, Ordering::SeqCst);
            return Err("http quota".into());
        }
        let result = if let Some(fixture) = fixture {
            // Offline fixture: policy on the requested URL only, no redirect walk.
            check_dev_target(url, &self.specs, false)?;
            reject_guest_headers(headers)?;
            Ok(HttpResponse {
                status: fixture.status,
                headers: fixture.headers.clone(),
                body: fixture.body.clone(),
            })
        } else {
            self.live_fetch(method, url, headers, body)
        };
        self.inflight_http.fetch_sub(1, Ordering::SeqCst);
        result
    }

    fn live_fetch(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<HttpResponse, String> {
        fetch_with_redirects(
            method,
            url,
            headers,
            body,
            |hop_url| check_dev_target(hop_url, &self.specs, false).map(|_| ()),
            |hop_url, method, headers, body| {
                let mut req = self.client.request(method.clone(), hop_url);
                for (k, v) in headers {
                    req = req.header(k, v);
                }
                if !body.is_empty() {
                    req = req.body(body.to_vec());
                }
                let resp = req.send().map_err(|err| err.to_string())?;
                let status = resp.status().as_u16();
                let location = resp
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                let headers = resp
                    .headers()
                    .iter()
                    .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                    .collect();
                let bytes = resp.bytes().map_err(|err| err.to_string())?;
                Ok(HopResponse {
                    status,
                    headers,
                    body: bytes.to_vec(),
                    location,
                })
            },
        )
    }

    pub fn ws_connect(&self, url: &str) -> Result<u32, String> {
        check_dev_target(url, &self.specs, false)?;
        {
            let sockets = self.sockets.lock().map_err(|err| err.to_string())?;
            if sockets.len() as u32 >= MAX_WS {
                return Err("ws quota".into());
            }
        }
        if let Some(frames) = &self.replay {
            return self.replay_connect(frames.clone());
        }
        self.live_connect(url)
    }

    fn replay_connect(&self, frames: Arc<[String]>) -> Result<u32, String> {
        let id = self.next_ws.fetch_add(1, Ordering::SeqCst);
        let closed = Arc::new(AtomicBool::new(false));
        let mailbox = self.mailbox.sender();
        let stop = Arc::clone(&self.stop);
        let closed_thread = Arc::clone(&closed);
        thread::Builder::new()
            .name(format!("ws-replay-{id}"))
            .spawn(move || {
                for text in frames.iter() {
                    if stop.load(Ordering::SeqCst) || closed_thread.load(Ordering::SeqCst) {
                        return;
                    }
                    let _ = mailbox.try_send(Mail::WsText {
                        handle: id,
                        text: text.clone(),
                    });
                }
            })
            .map_err(|err| err.to_string())?;
        self.sockets
            .lock()
            .map_err(|err| err.to_string())?
            .insert(id, WsSlot::Replay { closed });
        Ok(id)
    }

    fn live_connect(&self, url: &str) -> Result<u32, String> {
        let mut socket = connect_wss_pinned(url)?;
        match socket.get_mut() {
            MaybeTlsStream::Plain(stream) => {
                let _ = stream.set_read_timeout(Some(WS_READ_POLL));
                let _ = stream.set_write_timeout(Some(WS_WRITE_TIMEOUT));
            }
            MaybeTlsStream::NativeTls(stream) => {
                let tcp = stream.get_mut();
                let _ = tcp.set_read_timeout(Some(WS_READ_POLL));
                let _ = tcp.set_write_timeout(Some(WS_WRITE_TIMEOUT));
            }
            _ => {}
        }
        let shutdown = tcp_clone(&socket)?;
        let id = self.next_ws.fetch_add(1, Ordering::SeqCst);
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let mailbox = self.mailbox.sender();
        thread::Builder::new()
            .name(format!("ws-live-{id}"))
            .spawn(move || ws_worker(id, socket, cmd_rx, mailbox))
            .map_err(|err| err.to_string())?;
        self.sockets
            .lock()
            .map_err(|err| err.to_string())?
            .insert(
                id,
                WsSlot::Live {
                    cmds: cmd_tx,
                    shutdown,
                },
            );
        Ok(id)
    }

    pub fn ws_send(&self, handle: u32, message: &str) -> Result<(), String> {
        let sockets = self.sockets.lock().map_err(|err| err.to_string())?;
        match sockets.get(&handle) {
            Some(WsSlot::Replay { .. }) => Ok(()),
            Some(WsSlot::Live { cmds, .. }) => {
                let cmds = cmds.clone();
                drop(sockets);
                let (reply_tx, reply_rx) = mpsc::channel();
                cmds.send(WsCmd::Send {
                    text: message.to_string(),
                    reply: reply_tx,
                })
                .map_err(|_| "ws closed".to_string())?;
                reply_rx
                    .recv_timeout(WS_WRITE_TIMEOUT)
                    .map_err(|_| "ws write timeout".to_string())?
            }
            None => Err("no socket".into()),
        }
    }

    pub fn ws_close(&self, handle: u32) -> Result<(), String> {
        let mut sockets = self.sockets.lock().map_err(|err| err.to_string())?;
        if let Some(slot) = sockets.remove(&handle) {
            self.drop_slot(handle, slot);
        }
        Ok(())
    }

    pub fn close_all(&self) {
        let Ok(mut sockets) = self.sockets.lock() else {
            return;
        };
        for (handle, slot) in sockets.drain() {
            self.drop_slot(handle, slot);
        }
        drop(sockets);
        let _ = super::grpc::grpc_close_all(&self.grpc);
    }

    fn drop_slot(&self, handle: u32, slot: WsSlot) {
        match slot {
            WsSlot::Replay { closed } => {
                closed.store(true, Ordering::SeqCst);
                if !self.stop.load(Ordering::SeqCst) {
                    let _ = self.mailbox.sender().try_send(Mail::WsClosed(handle));
                }
            }
            WsSlot::Live { cmds, shutdown } => {
                let _ = cmds.send(WsCmd::Close);
                let _ = shutdown.shutdown(Shutdown::Both);
            }
        }
    }

}

fn ws_worker(
    handle: u32,
    mut socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    cmds: Receiver<WsCmd>,
    mailbox: std::sync::mpsc::SyncSender<Mail>,
) {
    let closed = || {
        let _ = mailbox.try_send(Mail::WsClosed(handle));
    };
    loop {
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                WsCmd::Send { text, reply } => {
                    let result = socket
                        .send(tungstenite::Message::Text(text.into()))
                        .map_err(|err| err.to_string());
                    let _ = reply.send(result);
                }
                WsCmd::Close => {
                    let _ = socket.close(None);
                    closed();
                    return;
                }
            }
        }
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                let _ = mailbox.try_send(Mail::WsText {
                    handle,
                    text: text.to_string(),
                });
            }
            Ok(tungstenite::Message::Binary(bin)) => {
                if let Ok(text) = String::from_utf8(bin.to_vec()) {
                    let _ = mailbox.try_send(Mail::WsText { handle, text });
                }
            }
            Ok(tungstenite::Message::Ping(p)) => {
                let _ = socket.send(tungstenite::Message::Pong(p));
            }
            Ok(tungstenite::Message::Pong(_)) | Ok(tungstenite::Message::Frame(_)) => {}
            Ok(tungstenite::Message::Close(_)) => {
                closed();
                return;
            }
            Err(err) if is_ws_timeout(&err) => {}
            Err(_) => {
                closed();
                return;
            }
        }
    }
}

fn is_ws_timeout(err: &tungstenite::Error) -> bool {
    match err {
        tungstenite::Error::Io(io) => {
            io.kind() == ErrorKind::WouldBlock || io.kind() == ErrorKind::TimedOut
        }
        _ => false,
    }
}

fn tcp_clone(socket: &tungstenite::WebSocket<MaybeTlsStream<TcpStream>>) -> Result<TcpStream, String> {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream.try_clone().map_err(|err| err.to_string()),
        MaybeTlsStream::NativeTls(stream) => {
            stream.get_ref().try_clone().map_err(|err| err.to_string())
        }
        _ => Err("no tcp for ws".into()),
    }
}

fn connect_wss_pinned(
    url: &str,
) -> Result<tungstenite::WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let (host, port) = https_url_host(url)?;
    let addrs = resolve_pinned(&host, port)?;
    let stream = dial_tcp(&addrs, HTTP_TIMEOUT)?;
    let _ = stream.set_nodelay(true);
    let connector =
        native_tls::TlsConnector::new().map_err(|err| format!("tls connector: {err}"))?;
    let tls = connector
        .connect(&host, stream)
        .map_err(|err| format!("tls: {err}"))?;
    let (socket, _) = tungstenite::client(url, MaybeTlsStream::NativeTls(tls))
        .map_err(|err| err.to_string())?;
    Ok(socket)
}

pub fn load_replay(path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("replay: {err}"))?;
    Ok(text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

pub fn load_http_fixtures(path: &Path) -> Result<HashMap<String, HttpFixture>, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("http-file: {err}"))?;
    parse_http_fixtures(&text)
}

pub fn parse_http_fixtures(text: &str) -> Result<HashMap<String, HttpFixture>, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|err| format!("http-file: {err}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "http-file: object required".to_string())?;
    let mut out = HashMap::new();
    for (url, spec) in obj {
        let spec = spec
            .as_object()
            .ok_or_else(|| format!("http-file: {url} is not an object"))?;
        let status = spec
            .get("status")
            .and_then(Value::as_u64)
            .unwrap_or(200) as u16;
        let headers = match spec.get("headers") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| {
                    let pair = item.as_array()?;
                    Some((
                        pair.first()?.as_str()?.to_string(),
                        pair.get(1)?.as_str()?.to_string(),
                    ))
                })
                .collect(),
            Some(_) => return Err(format!("http-file: {url} headers")),
        };
        let body = match spec.get("body") {
            None => Vec::new(),
            Some(Value::String(text)) => text.as_bytes().to_vec(),
            Some(other) => serde_json::to_vec(other).map_err(|err| format!("http-file: {err}"))?,
        };
        if body.len() > MAX_BODY {
            return Err("response too large".into());
        }
        out.insert(
            url_without_query(url),
            HttpFixture {
                status,
                headers,
                body,
            },
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_json_body_fixture() {
        let map = parse_http_fixtures(
            r#"{
              "https://api.twitch.tv/helix/users?foo=1": {
                "status": 200,
                "body": {"data":[{"id":"1","login":"fixture"}]}
              }
            }"#,
        )
        .unwrap();
        let fix = map.get("https://api.twitch.tv/helix/users").unwrap();
        assert_eq!(fix.status, 200);
        let body = String::from_utf8_lossy(&fix.body);
        assert!(body.contains("fixture"), "{body}");
    }

    #[test]
    fn reject_guest_hop_by_hop_headers() {
        let err = reject_guest_headers(&[("Host".into(), "evil".into())]).unwrap_err();
        assert!(err.contains("forbidden header"), "{err}");
        let err = reject_guest_headers(&[("Content-Length".into(), "1".into())]).unwrap_err();
        assert!(err.contains("forbidden header"), "{err}");
        assert!(reject_guest_headers(&[("X-Api-Key".into(), "k".into())]).is_ok());
    }

    #[test]
    fn redirect_out_of_manifest_never_sends_second_hop() {
        let sends = Arc::new(AtomicUsize::new(0));
        let sends_c = Arc::clone(&sends);
        let err = fetch_with_redirects(
            "GET",
            "https://a.example/start",
            &[("X-Api-Key".into(), "secret".into())],
            &[],
            |url| {
                if url.contains("evil.example") {
                    Err("host evil.example not in manifest".into())
                } else {
                    Ok(())
                }
            },
            |url, _method, _headers, _body| {
                sends_c.fetch_add(1, Ordering::SeqCst);
                assert!(url.contains("a.example"), "{url}");
                Ok(HopResponse {
                    status: 302,
                    headers: vec![],
                    body: b"nope".to_vec(),
                    location: Some("https://evil.example/leak".into()),
                })
            },
        )
        .unwrap_err();
        assert!(err.contains("not in manifest"), "{err}");
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn redirect_private_a_no_tcp_on_second_hop() {
        use std::net::{Ipv4Addr, SocketAddr};

        let sends = Arc::new(AtomicUsize::new(0));
        let dialed = Arc::new(Mutex::new(Vec::<SocketAddr>::new()));
        let sends_c = Arc::clone(&sends);
        let dialed_c = Arc::clone(&dialed);

        crate::hosts::with_net_hooks(
            |_host, port| Ok(vec![SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), port))]),
            Some(Box::new(move |addrs| {
                dialed_c.lock().unwrap().extend_from_slice(addrs);
                Err("dial blocked in test".into())
            })),
            || {
                let err = fetch_with_redirects(
                    "GET",
                    "https://a.example/start",
                    &[],
                    &[],
                    |_| Ok(()),
                    |_url, _method, _headers, _body| {
                        let n = sends_c.fetch_add(1, Ordering::SeqCst);
                        if n == 0 {
                            Ok(HopResponse {
                                status: 302,
                                headers: vec![],
                                body: Vec::new(),
                                location: Some("https://priv.example/".into()),
                            })
                        } else {
                            let addrs = resolve_pinned("priv.example", 443)?;
                            let _ = dial_tcp(&addrs, std::time::Duration::from_millis(50))?;
                            Ok(HopResponse {
                                status: 200,
                                headers: vec![],
                                body: b"ok".to_vec(),
                                location: None,
                            })
                        }
                    },
                )
                .unwrap_err();
                assert!(err.contains("forbidden address"), "{err}");
            },
        );

        assert_eq!(sends.load(Ordering::SeqCst), 2);
        assert!(dialed.lock().unwrap().is_empty(), "must not dial private A");
    }

    #[test]
    fn redirect_strips_guest_headers_on_cross_origin() {
        let seen = Arc::new(Mutex::new(Vec::<usize>::new()));
        let seen_c = Arc::clone(&seen);
        let resp = fetch_with_redirects(
            "GET",
            "https://a.example/start",
            &[
                ("X-Api-Key".into(), "secret".into()),
                ("Authorization".into(), "Bearer tok".into()),
            ],
            &[],
            |_| Ok(()),
            |url, _method, headers, _body| {
                seen_c.lock().unwrap().push(headers.len());
                if url.contains("a.example") {
                    Ok(HopResponse {
                        status: 302,
                        headers: vec![],
                        body: Vec::new(),
                        location: Some("https://b.example/next".into()),
                    })
                } else {
                    Ok(HopResponse {
                        status: 200,
                        headers: vec![],
                        body: b"ok".to_vec(),
                        location: None,
                    })
                }
            },
        )
        .unwrap();
        assert_eq!(resp.body, b"ok");
        assert_eq!(*seen.lock().unwrap(), vec![2, 0]);
    }

    #[test]
    fn redirect_chain_too_long() {
        let err = fetch_with_redirects(
            "GET",
            "https://a.example/0",
            &[],
            &[],
            |_| Ok(()),
            |_url, _method, _headers, _body| {
                Ok(HopResponse {
                    status: 302,
                    headers: vec![],
                    body: b"x".to_vec(),
                    location: Some("https://a.example/n".into()),
                })
            },
        )
        .unwrap_err();
        assert!(err.contains("redirect chain too long"), "{err}");
    }

    #[test]
    fn grpc_metadata_rejects_pseudo_and_grpc_prefix() {
        let err = super::super::grpc::validate_guest_metadata(&[(":path".into(), "/x".into())])
            .unwrap_err();
        assert!(err.contains("forbidden header"), "{err}");
        let err = super::super::grpc::validate_guest_metadata(&[("grpc-timeout".into(), "1S".into())])
            .unwrap_err();
        assert!(err.contains("forbidden header"), "{err}");
        assert!(super::super::grpc::validate_guest_metadata(&[(
            "authorization".into(),
            "Bearer x".into()
        )])
        .is_ok());
    }
}
