use super::net::DevNet;
use crate::hosts::{allowed_by_manifest, https_url_host, is_blocked_name, HostSpec};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use url::Url;

const MAX_FILE: usize = 512 * 1024;
const MAX_AUDIO: usize = 4 * 1024 * 1024;
const AUDIO_MPEG: &str = "audio/mpeg";

pub struct DevCache {
    blobs: Mutex<HashMap<String, (String, Vec<u8>)>>,
    pins: Mutex<HashMap<String, Vec<String>>>,
}

impl DevCache {
    pub fn new() -> Self {
        Self {
            blobs: Mutex::new(HashMap::new()),
            pins: Mutex::new(HashMap::new()),
        }
    }

    pub fn lookup(&self, specs: &[HostSpec], url: &str) -> Option<String> {
        let norm = normalize_url(url).ok()?;
        host_ok(&norm, specs).ok()?;
        let key = url_key(&norm);
        let map = self.blobs.lock().ok()?;
        map.contains_key(&key).then_some(key)
    }

    pub fn ensure(
        &self,
        net: &DevNet,
        specs: &[HostSpec],
        has_http: bool,
        url: &str,
    ) -> Result<String, String> {
        let norm = normalize_url(url)?;
        host_ok(&norm, specs)?;
        let key = url_key(&norm);
        if self.blobs.lock().map_err(|err| err.to_string())?.contains_key(&key) {
            self.pin("dev", &key)?;
            return Ok(key);
        }
        if !has_http {
            return Err("no grant net.http".into());
        }
        let resp = net.fetch("GET", &norm, &[], &[])?;
        if resp.status != 200 {
            return Err(format!("http {}", resp.status));
        }
        let mime = detect_mime(&resp.headers, &resp.body)?;
        if resp.body.len() > max_for_mime(mime) {
            return Err("file too large".into());
        }
        self.store("dev", &key, mime, resp.body)?;
        Ok(key)
    }

    pub fn put(&self, plugin_id: &str, mime: &str, bytes: &[u8]) -> Result<String, String> {
        let declared = parse_cache_mime(mime)?;
        if declared != AUDIO_MPEG {
            return Err("put audio/mpeg only".into());
        }
        let sniffed = sniff(bytes)?;
        if sniffed != declared {
            return Err("file type mismatch".into());
        }
        if bytes.len() > MAX_AUDIO {
            return Err("file too large".into());
        }
        let sha = hex(Sha256::digest(bytes));
        let prefix: String = sha.chars().take(16).collect();
        let norm = format!("modus-audio://{plugin_id}/{prefix}");
        let key = url_key(&norm);
        self.store(plugin_id, &key, AUDIO_MPEG, bytes.to_vec())?;
        Ok(key)
    }

    pub fn release(&self, key: &str) -> Result<(), String> {
        if !is_cache_key(key) {
            return Ok(());
        }
        self.pins
            .lock()
            .map_err(|err| err.to_string())?
            .remove(key);
        self.blobs
            .lock()
            .map_err(|err| err.to_string())?
            .remove(key);
        Ok(())
    }

    fn store(
        &self,
        plugin_id: &str,
        key: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.blobs
            .lock()
            .map_err(|err| err.to_string())?
            .insert(key.to_string(), (mime.to_string(), bytes));
        self.pin(plugin_id, key)
    }

    fn pin(&self, plugin_id: &str, key: &str) -> Result<(), String> {
        let mut pins = self.pins.lock().map_err(|err| err.to_string())?;
        let entry = pins.entry(key.to_string()).or_default();
        if !entry.iter().any(|id| id == plugin_id) {
            entry.push(plugin_id.to_string());
        }
        Ok(())
    }
}

fn is_cache_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn max_for_mime(mime: &str) -> usize {
    if mime == AUDIO_MPEG {
        MAX_AUDIO
    } else {
        MAX_FILE
    }
}

fn host_ok(url: &str, specs: &[HostSpec]) -> Result<(), String> {
    let (host, port) = https_url_host(url)?;
    if is_blocked_name(&host) {
        return Err(format!("forbidden address {host}"));
    }
    if !allowed_by_manifest(&host, port, specs) {
        return Err(format!("host {host} not in manifest"));
    }
    Ok(())
}

fn normalize_url(raw: &str) -> Result<String, String> {
    let parsed = Url::parse(raw).map_err(|err| format!("url: {err}"))?;
    if parsed.scheme() != "https" {
        return Err("https only".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("userinfo forbidden".into());
    }
    let (host, port) = https_url_host(raw)?;
    if is_blocked_name(&host) {
        return Err(format!("forbidden address {host}"));
    }
    let mut out = format!("https://{host}");
    if port != 443 {
        out.push(':');
        out.push_str(&port.to_string());
    }
    let path = parsed.path();
    if path.is_empty() {
        out.push('/');
    } else {
        out.push_str(path);
    }
    if let Some(query) = parsed.query() {
        out.push('?');
        out.push_str(query);
    }
    Ok(out)
}

fn url_key(norm: &str) -> String {
    hex(Sha256::digest(norm.as_bytes()))
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn detect_mime(headers: &[(String, String)], body: &[u8]) -> Result<&'static str, String> {
    let sniffed = sniff(body)?;
    if let Some((_, raw)) = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        let declared = parse_cache_mime(raw)?;
        if declared != sniffed {
            return Err("file type mismatch".into());
        }
        Ok(declared)
    } else {
        Ok(sniffed)
    }
}

fn parse_cache_mime(raw: &str) -> Result<&'static str, String> {
    let main = raw
        .split(';')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase();
    match main.as_str() {
        "image/png" => Ok("image/png"),
        "image/jpeg" | "image/jpg" => Ok("image/jpeg"),
        "image/gif" => Ok("image/gif"),
        "image/webp" => Ok("image/webp"),
        "image/svg+xml" => Ok("image/svg+xml"),
        "audio/mpeg" | "audio/mp3" => Ok(AUDIO_MPEG),
        _ => Err("unsupported MIME".into()),
    }
}

fn sniff(body: &[u8]) -> Result<&'static str, String> {
    if body.len() >= 8 && body.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok("image/png");
    }
    if body.len() >= 3 && body[0] == 0xff && body[1] == 0xd8 && body[2] == 0xff {
        return Ok("image/jpeg");
    }
    if body.len() >= 6 && (body.starts_with(b"GIF87a") || body.starts_with(b"GIF89a")) {
        return Ok("image/gif");
    }
    if body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP" {
        return Ok("image/webp");
    }
    if let Ok(text) = std::str::from_utf8(body) {
        let lower = text
            .trim_start_matches('\u{feff}')
            .trim_start()
            .to_ascii_lowercase();
        if !lower.contains("<script")
            && (lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg")))
        {
            return Ok("image/svg+xml");
        }
    }
    if is_mpeg_audio(body) {
        return Ok(AUDIO_MPEG);
    }
    Err("unsupported MIME".into())
}

fn is_mpeg_audio(body: &[u8]) -> bool {
    if body.len() >= 3 && body.starts_with(b"ID3") {
        return true;
    }
    if body.len() < 2 {
        return false;
    }
    body[0] == 0xff && (body[1] & 0xe0) == 0xe0 && (body[1] & 0x18) != 0x08
}
