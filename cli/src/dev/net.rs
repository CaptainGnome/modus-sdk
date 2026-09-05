use super::mailbox::{Mail, Mailbox};
use crate::hosts::{check_dev_target, https_url_host, url_without_query, HostSpec};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;
const MAX_BODY: usize = 4 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const WS_WRITE_TIMEOUT: Duration = Duration::from_secs(15);
const WS_READ_POLL: Duration = Duration::from_millis(50);
const MAX_HTTP: u32 = 4;
const MAX_WS: u32 = 2;

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
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    return attempt.error("слишком длинная цепочка редиректов");
                }
                if https_url_host(attempt.url().as_str()).is_err() {
                    return attempt.error("редирект не https");
                }
                attempt.follow()
            }))
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
        }))
    }

    pub fn fetch(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
    ) -> Result<HttpResponse, String> {
        if body.len() > MAX_BODY {
            return Err("тело слишком большое".into());
        }
        let key = url_without_query(url);
        let fixture = self.http_fixtures.get(&key);
        check_dev_target(url, &self.specs, fixture.is_none())?;
        let current = self.inflight_http.fetch_add(1, Ordering::SeqCst);
        if current >= MAX_HTTP {
            self.inflight_http.fetch_sub(1, Ordering::SeqCst);
            return Err("квота http".into());
        }
        let result = if let Some(fixture) = fixture {
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
        let mut req = self.client.request(
            method
                .parse::<reqwest::Method>()
                .map_err(|_| "неверный метод".to_string())?,
            url,
        );
        for (k, v) in headers {
            req = req.header(k, v);
        }
        if !body.is_empty() {
            req = req.body(body.to_vec());
        }
        let resp = req.send().map_err(|err| err.to_string())?;
        let final_url = resp.url().clone();
        check_dev_target(final_url.as_str(), &self.specs, true)?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
            .collect();
        let bytes = resp.bytes().map_err(|err| err.to_string())?;
        if bytes.len() > MAX_BODY {
            return Err("ответ слишком большой".into());
        }
        Ok(HttpResponse {
            status,
            headers,
            body: bytes.to_vec(),
        })
    }

    pub fn ws_connect(&self, url: &str) -> Result<u32, String> {
        check_dev_target(url, &self.specs, self.replay.is_none())?;
        {
            let sockets = self.sockets.lock().map_err(|err| err.to_string())?;
            if sockets.len() as u32 >= MAX_WS {
                return Err("квота ws".into());
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
        let (mut socket, _) = tungstenite::connect(url).map_err(|err| err.to_string())?;
        match socket.get_mut() {
            tungstenite::stream::MaybeTlsStream::Plain(stream) => {
                let _ = stream.set_read_timeout(Some(WS_READ_POLL));
                let _ = stream.set_write_timeout(Some(WS_WRITE_TIMEOUT));
            }
            tungstenite::stream::MaybeTlsStream::NativeTls(stream) => {
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
                .map_err(|_| "ws закрыт".to_string())?;
                reply_rx
                    .recv_timeout(WS_WRITE_TIMEOUT)
                    .map_err(|_| "ws write timeout".to_string())?
            }
            None => Err("нет сокета".into()),
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

fn tcp_clone(
    socket: &tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
) -> Result<TcpStream, String> {
    match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(stream) => {
            stream.try_clone().map_err(|err| err.to_string())
        }
        tungstenite::stream::MaybeTlsStream::NativeTls(stream) => stream
            .get_ref()
            .try_clone()
            .map_err(|err| err.to_string()),
        _ => Err("нет tcp для ws".into()),
    }
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
        .ok_or_else(|| "http-file: нужен объект".to_string())?;
    let mut out = HashMap::new();
    for (url, spec) in obj {
        let spec = spec
            .as_object()
            .ok_or_else(|| format!("http-file: {url} не объект"))?;
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
            return Err("ответ слишком большой".into());
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
}
