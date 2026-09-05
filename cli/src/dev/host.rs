use super::act::{ActJob, ActKind, ActRequest};
use super::bus::Bus;
use super::event::parse_opaque;
use super::history::HistoryGate;
use super::kv::DevKv;
use super::mailbox::{Mail, Mailbox};
use super::media::DevCache;
use super::net::DevNet;
use super::settings::DevSettings;
use super::wit_map::{map_payload, to_wit_event};
use crate::hosts::HostSpec;
use std::path::PathBuf;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wasmtime::StoreLimits;

wasmtime::component::bindgen!({
    path: "../wit",
    world: "runtime",
});

pub(crate) struct HostData {
    pub(crate) plugin_id: String,
    pub(crate) version: String,
    pub(crate) platform_id: Option<String>,
    pub(crate) plugin_dir: PathBuf,
    pub(crate) has_emit: bool,
    pub(crate) has_auth: bool,
    pub(crate) has_http: bool,
    pub(crate) has_ws: bool,
    pub(crate) has_alert: bool,
    pub(crate) has_kv: bool,
    pub(crate) has_chat: bool,
    pub(crate) has_cache: bool,
    pub(crate) has_catalog: bool,
    pub(crate) has_ui: bool,
    pub(crate) has_history: bool,
    pub(crate) has_audio: bool,
    pub(crate) has_bridge: bool,
    pub(crate) has_embed: bool,
    pub(crate) has_rates: bool,
    pub(crate) has_rates_convert: bool,
    pub(crate) embed_specs: Vec<HostSpec>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) mailbox: Arc<Mailbox>,
    pub(crate) bus: Arc<Bus>,
    pub(crate) history: Arc<HistoryGate>,
    pub(crate) net: Arc<DevNet>,
    pub(crate) cache: Arc<DevCache>,
    pub(crate) kv: Arc<DevKv>,
    pub(crate) settings: Arc<DevSettings>,
    pub(crate) specs: Vec<HostSpec>,
    pub(crate) auth_account: String,
    pub(crate) access_token: Option<String>,
    pub(crate) limits: StoreLimits,
}

impl HostData {
    fn halted(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn running(&self) -> Result<(), String> {
        if self.halted() {
            Err("остановлен".into())
        } else {
            Ok(())
        }
    }
}

impl modus::abi::types::Host for HostData {}

impl modus::abi::clock::Host for HostData {
    fn sleep_ms(&mut self, ms: u32) {
        let mut left = ms;
        while left > 0 {
            if self.halted() {
                return;
            }
            let chunk = left.min(50);
            thread::sleep(Duration::from_millis(chunk as u64));
            left -= chunk;
        }
    }
}

impl modus::abi::self_info::Host for HostData {
    fn plugin_id(&mut self) -> String {
        self.plugin_id.clone()
    }

    fn version(&mut self) -> String {
        self.version.clone()
    }
}

impl modus::abi::log::Host for HostData {
    fn log(&mut self, level: modus::abi::log::Level, message: String) {
        eprintln!("[{level:?}] {}: {message}", self.plugin_id);
        let _ = io::stderr().flush();
    }
}

impl modus::abi::wait::Host for HostData {
    fn subscribe(&mut self) {
        if self.halted() {
            return;
        }
        self.bus.subscribe();
    }

    fn set_timer(&mut self, ms: u32) {
        if self.halted() {
            return;
        }
        self.mailbox.set_timer(ms);
    }

    fn wait(&mut self) -> modus::abi::wait::Ready {
        if self.halted() {
            return modus::abi::wait::Ready::Stop;
        }
        match self.mailbox.wait(&self.stop) {
            Mail::Stop => modus::abi::wait::Ready::Stop,
            Mail::Timer => modus::abi::wait::Ready::Timer,
            Mail::WsText { handle, text } => {
                modus::abi::wait::Ready::WsText(modus::abi::wait::WsFrame { handle, text })
            }
            Mail::WsClosed(handle) => {
                if self.halted() {
                    modus::abi::wait::Ready::Stop
                } else {
                    modus::abi::wait::Ready::WsClosed(handle)
                }
            }
            Mail::Bus(event) => modus::abi::wait::Ready::Bus(to_wit_event(event)),
            Mail::Ui(payload) => modus::abi::wait::Ready::Ui(payload),
            Mail::Settings => modus::abi::wait::Ready::Settings,
            Mail::Act(req) => modus::abi::wait::Ready::Act(to_wit_act(req)),
            Mail::MediaEnded(id) => {
                if self.has_audio {
                    modus::abi::wait::Ready::MediaEnded(id)
                } else {
                    return self.wait();
                }
            }
        }
    }
}

impl modus::abi::bus_emit::Host for HostData {
    fn emit(
        &mut self,
        channel: String,
        payload: modus::abi::types::Payload,
        opaque: Option<String>,
    ) -> Result<(), String> {
        self.running()?;
        if !self.has_emit {
            return Err("нет гранта bus.emit".into());
        }
        let mapped = map_payload(payload)?;
        let opaque = parse_opaque(opaque)?;
        self.bus.emit_from_plugin(
            self.plugin_id.clone(),
            self.platform_id.clone(),
            channel,
            mapped,
            opaque,
        )
    }
}

impl modus::abi::alert_enqueue::Host for HostData {
    fn enqueue(&mut self, job: modus::abi::alert_enqueue::Job) -> Result<String, String> {
        self.running()?;
        if !self.has_alert {
            return Err("нет гранта alert.enqueue".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let _ = writeln!(
            io::stderr(),
            "alert enqueue {id} {:?} event={} duration={} title={}",
            job.priority,
            job.event_id,
            job.duration_ms,
            job.title
        );
        Ok(id)
    }

    fn complete(
        &mut self,
        job_id: String,
        outcome: Result<(), String>,
    ) -> Result<(), String> {
        self.running()?;
        if !self.has_alert {
            return Err("нет гранта alert.enqueue".into());
        }
        match outcome {
            Ok(()) => {
                let _ = writeln!(io::stderr(), "alert complete {job_id} ok");
            }
            Err(err) => {
                let _ = writeln!(io::stderr(), "alert complete {job_id} err {err}");
            }
        }
        Ok(())
    }
}

impl modus::abi::settings::Host for HostData {
    fn get(&mut self, key: String) -> Option<String> {
        self.settings.get(&key)
    }

    fn set_label(&mut self, key: String, text: String) -> Result<(), String> {
        self.running()?;
        self.settings.set_label(&key, &text)?;
        let _ = writeln!(io::stderr(), "settings set-label {key}");
        Ok(())
    }

    fn set_label_i18n(
        &mut self,
        key: String,
        label_key: String,
        params: Option<String>,
    ) -> Result<(), String> {
        self.running()?;
        self.settings
            .set_label_i18n(&key, &label_key, params)?;
        let _ = writeln!(io::stderr(), "settings set-label-i18n {key}");
        Ok(())
    }
}

fn validate_dev_asset_path(rel: &str) -> Result<(), String> {
    if rel.is_empty()
        || rel.contains("..")
        || rel.contains('\\')
        || rel.starts_with('/')
        || rel.starts_with("assets/")
    {
        return Err("небезопасный путь assets".into());
    }
    if rel
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("небезопасный путь assets".into());
    }
    Ok(())
}

impl modus::abi::assets::Host for HostData {
    fn read(&mut self, path: String) -> Result<Vec<u8>, String> {
        validate_dev_asset_path(&path)?;
        let file_path = self.plugin_dir.join("assets").join(&path);
        let bytes = std::fs::read(&file_path)
            .map_err(|_| format!("нет assets/{path}"))?;
        if bytes.len() > 512 * 1024 {
            return Err("файл assets слишком большой".into());
        }
        Ok(bytes)
    }
}

impl modus::abi::history_read::Host for HostData {
    fn read(
        &mut self,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<modus::abi::history_read::Page, String> {
        self.running()?;
        if !self.has_history {
            return Err("нет гранта history.read".into());
        }
        self.history.touch(&self.plugin_id)?;
        let (events, next) = self.bus.history_page(cursor.as_deref(), limit)?;
        Ok(modus::abi::history_read::Page {
            events: events.into_iter().map(to_wit_event).collect(),
            next,
            alert_shown: Vec::new(),
        })
    }
}

impl modus::abi::storage_kv::Host for HostData {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        self.running()?;
        if !self.has_kv {
            return Err("нет гранта storage.kv".into());
        }
        self.kv.get(&key)
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        self.running()?;
        if !self.has_kv {
            return Err("нет гранта storage.kv".into());
        }
        self.kv.set(&key, &value)
    }

    fn delete(&mut self, key: String) -> Result<(), String> {
        self.running()?;
        if !self.has_kv {
            return Err("нет гранта storage.kv".into());
        }
        self.kv.delete(&key)
    }

    fn list_keys(&mut self, prefix: String) -> Result<Vec<String>, String> {
        self.running()?;
        if !self.has_kv {
            return Err("нет гранта storage.kv".into());
        }
        self.kv.list_keys(&prefix)
    }
}

impl modus::abi::chat_act::Host for HostData {
    fn act(&mut self, job: modus::abi::chat_act::ActJob) -> Result<String, String> {
        self.running()?;
        if !self.has_chat {
            return Err("нет гранта chat.act".into());
        }
        let mut job = ActJob {
            platform: job.platform,
            channel: job.channel,
            kind: from_wit_act_kind(job.kind),
            text: job.text,
            message_id: job.message_id,
            target_user_id: job.target_user_id,
            duration_sec: job.duration_sec,
        };
        job.validate()?;
        let id = uuid::Uuid::new_v4().to_string();
        let _ = writeln!(
            io::stderr(),
            "chat.act {id} {} {}/{} {}",
            job.kind_label(),
            job.platform,
            job.channel,
            job.text.as_deref().unwrap_or("")
        );
        Ok(id)
    }
}

impl modus::abi::chat_complete::Host for HostData {
    fn complete(&mut self, id: String, outcome: Result<(), String>) {
        match outcome {
            Ok(()) => {
                let _ = writeln!(io::stderr(), "chat.complete {id} ok");
            }
            Err(err) => {
                let _ = writeln!(io::stderr(), "chat.complete {id} err {err}");
            }
        }
    }
}

impl modus::abi::auth_token::Host for HostData {
    fn list_accounts(&mut self) -> Vec<String> {
        if self.halted() || !self.has_auth || self.access_token.is_none() {
            return Vec::new();
        }
        vec![self.auth_account.clone()]
    }

    fn token(&mut self, account_id: String) -> Result<String, String> {
        self.running()?;
        if !self.has_auth {
            return Err("нет гранта auth.token".into());
        }
        let Some(token) = &self.access_token else {
            return Err("чужой аккаунт".into());
        };
        if account_id != self.auth_account {
            return Err("чужой аккаунт".into());
        }
        Ok(token.clone())
    }
}

impl modus::abi::net_http::Host for HostData {
    fn fetch(
        &mut self,
        method: String,
        url: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<modus::abi::net_http::HttpResponse, String> {
        self.running()?;
        if !self.has_http {
            return Err("нет гранта net.http".into());
        }
        let resp = self.net.fetch(&method, &url, &headers, &body)?;
        Ok(modus::abi::net_http::HttpResponse {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
        })
    }
}

impl modus::abi::net_ws::Host for HostData {
    fn connect(&mut self, url: String) -> Result<u32, String> {
        self.running()?;
        if !self.has_ws {
            return Err("нет гранта net.ws".into());
        }
        self.net.ws_connect(&url)
    }

    fn send_text(&mut self, handle: u32, message: String) -> Result<(), String> {
        self.running()?;
        if !self.has_ws {
            return Err("нет гранта net.ws".into());
        }
        self.net.ws_send(handle, &message)
    }

    fn close(&mut self, handle: u32) -> Result<(), String> {
        if !self.has_ws {
            return Err("нет гранта net.ws".into());
        }
        self.net.ws_close(handle)
    }
}

impl modus::abi::media_cache::Host for HostData {
    fn lookup(&mut self, url: String) -> Option<String> {
        if self.halted() || !self.has_cache {
            return None;
        }
        self.cache.lookup(&self.specs, &url)
    }

    fn ensure(&mut self, url: String) -> Result<String, String> {
        self.running()?;
        if !self.has_cache {
            return Err("нет гранта media.cache".into());
        }
        self.cache
            .ensure(&self.net, &self.specs, self.has_http, &url)
    }

    fn put(&mut self, mime: String, bytes: Vec<u8>) -> Result<String, String> {
        self.running()?;
        if !self.has_cache {
            return Err("нет гранта media.cache".into());
        }
        self.cache.put(&self.plugin_id, &mime, &bytes)
    }

    fn release(&mut self, key: String) -> Result<(), String> {
        self.running()?;
        if !self.has_cache {
            return Err("нет гранта media.cache".into());
        }
        self.cache.release(&key)
    }
}

impl modus::abi::media_audio::Host for HostData {
    fn play(&mut self, spec: modus::abi::media_audio::Spec) -> Result<String, String> {
        self.running()?;
        if !self.has_audio {
            return Err("нет гранта media.audio".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let label = match &spec {
            modus::abi::media_audio::Spec::Asset(path) => format!("asset:{path}"),
            modus::abi::media_audio::Spec::Url(url) => format!("url:{url}"),
            modus::abi::media_audio::Spec::Tts(text) => {
                format!("tts:{}", text.chars().take(40).collect::<String>())
            }
        };
        let _ = writeln!(io::stderr(), "media.audio play {id} {label}");
        let mailbox = Arc::clone(&self.mailbox);
        let ended = id.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            mailbox.wake_media_ended(ended);
        });
        Ok(id)
    }

    fn stop(&mut self, id: String) -> Result<(), String> {
        self.running()?;
        if !self.has_audio {
            return Err("нет гранта media.audio".into());
        }
        let _ = writeln!(io::stderr(), "media.audio stop {id}");
        self.mailbox.wake_media_ended(id);
        Ok(())
    }
}

impl modus::abi::bridge::Host for HostData {
    fn invoke(
        &mut self,
        id: String,
        request_type: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        self.running()?;
        if !self.has_bridge {
            return Err("нет гранта bridge.obs".into());
        }
        if id != "obs" {
            return Err(format!("неизвестный bridge id {id}"));
        }
        let _ = writeln!(
            io::stderr(),
            "bridge invoke {id} {request_type} ({} bytes)",
            payload.len()
        );
        Err("нет соединения".into())
    }
}

impl modus::abi::media_embed::Host for HostData {
    fn hosts(&mut self) -> Vec<String> {
        if !self.has_embed {
            return Vec::new();
        }
        self.embed_specs.iter().map(|s| s.canonical()).collect()
    }

    fn allowed(&mut self, url: String) -> bool {
        if !self.has_embed {
            return false;
        }
        let Ok((host, port)) = crate::hosts::https_url_host(&url) else {
            return false;
        };
        if crate::hosts::is_blocked_name(&host) {
            return false;
        }
        crate::hosts::allowed_by_manifest(&host, port, &self.embed_specs)
    }
}

impl modus::abi::catalog::Host for HostData {
    fn publish(&mut self, name: String, payload: Vec<u8>) -> Result<(), String> {
        self.running()?;
        if !self.has_catalog {
            return Err("нет гранта catalog.publish".into());
        }
        let text = String::from_utf8_lossy(&payload);
        let _ = writeln!(
            io::stderr(),
            "catalog publish {name} ({} bytes)\n{text}",
            payload.len()
        );
        Ok(())
    }

    fn unpublish(&mut self, name: String) -> Result<(), String> {
        self.running()?;
        if !self.has_catalog {
            return Err("нет гранта catalog.publish".into());
        }
        let _ = writeln!(io::stderr(), "catalog unpublish {name}");
        Ok(())
    }
}

impl modus::abi::rates_publish::Host for HostData {
    fn publish(
        &mut self,
        rates: Vec<modus::abi::rates_publish::Rate>,
    ) -> Result<(), String> {
        self.running()?;
        if !self.has_rates {
            return Err("нет гранта rates.publish".into());
        }
        let _ = writeln!(io::stderr(), "rates publish {} pairs", rates.len());
        for rate in &rates {
            let _ = writeln!(
                io::stderr(),
                "  {}→{} = {}",
                rate.from, rate.to, rate.value
            );
        }
        Ok(())
    }
}

impl modus::abi::rates::Host for HostData {
    fn base(&mut self) -> String {
        if self.halted() || !self.has_rates_convert {
            return String::new();
        }
        "RUB".into()
    }

    fn convert_to_base(&mut self, amount: f64, from: String) -> Result<f64, String> {
        self.running()?;
        if !self.has_rates_convert {
            return Err("нет гранта rates.convert".into());
        }
        let from = from.trim().to_ascii_uppercase();
        if from == "RUB" {
            let scale = 100f64;
            return Ok((amount * scale).floor() / scale);
        }
        Err(format!("нет курса {from}→RUB"))
    }
}

impl modus::abi::ui_slot::Host for HostData {
    fn post(&mut self, payload: Vec<u8>) -> Result<(), String> {
        self.running()?;
        if !self.has_ui {
            return Err("нет слота ui".into());
        }
        let text = String::from_utf8_lossy(&payload);
        let _ = writeln!(
            io::stderr(),
            "ui-slot post ({} bytes)\n{text}",
            payload.len()
        );
        Ok(())
    }
}

fn to_wit_act(req: ActRequest) -> modus::abi::wait::ActRequest {
    modus::abi::wait::ActRequest {
        id: req.id,
        account_id: req.account_id,
        platform: req.platform,
        channel: req.channel,
        kind: to_wit_act_kind(req.kind),
        text: req.text,
        message_id: req.message_id,
        target_user_id: req.target_user_id,
        duration_sec: req.duration_sec,
    }
}

fn to_wit_act_kind(kind: ActKind) -> modus::abi::wait::ActKind {
    match kind {
        ActKind::Send => modus::abi::wait::ActKind::Send,
        ActKind::Delete => modus::abi::wait::ActKind::Delete,
        ActKind::Timeout => modus::abi::wait::ActKind::Timeout,
        ActKind::Ban => modus::abi::wait::ActKind::Ban,
        ActKind::Unban => modus::abi::wait::ActKind::Unban,
    }
}

fn from_wit_act_kind(kind: modus::abi::wait::ActKind) -> ActKind {
    match kind {
        modus::abi::wait::ActKind::Send => ActKind::Send,
        modus::abi::wait::ActKind::Delete => ActKind::Delete,
        modus::abi::wait::ActKind::Timeout => ActKind::Timeout,
        modus::abi::wait::ActKind::Ban => ActKind::Ban,
        modus::abi::wait::ActKind::Unban => ActKind::Unban,
    }
}
