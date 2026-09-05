use crate::manifest::is_plugin_id;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Role {
    Consumer,
    Emitter,
    Connector,
    Provider,
    Widget,
    Panel,
    Reader,
    Player,
    Bridge,
    Embedder,
    Rates,
    Alerter,
    Commander,
    Store,
}

impl Role {
    fn feature(self) -> &'static str {
        match self {
            Self::Consumer => "consumer",
            Self::Emitter => "emitter",
            Self::Connector => "connector",
            Self::Provider => "provider",
            Self::Widget | Self::Panel => "widget",
            Self::Reader => "reader",
            Self::Player => "player",
            Self::Bridge => "bridge",
            Self::Embedder => "embedder",
            Self::Rates => "rates",
            Self::Alerter => "alerter",
            Self::Commander => "commander",
            Self::Store => "store",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum PanelMode {
    Native,
    Web,
}

pub struct NewArgs {
    pub role: Role,
    pub id: String,
    pub name: Option<String>,
    pub author: Option<String>,
    pub dir: Option<PathBuf>,
    pub lang: Option<String>,
    pub mode: Option<PanelMode>,
}

pub fn scaffold(args: NewArgs) -> Result<PathBuf, String> {
    if let Some(lang) = args.lang.as_deref() {
        if !lang.eq_ignore_ascii_case("rust") {
            return Err("S1 только Rust".into());
        }
    }
    if args.mode.is_some() && args.role != Role::Panel {
        return Err("mode только для panel".into());
    }
    if !is_plugin_id(&args.id) {
        return Err("plugin id: нужен reverse-DNS (com.publisher.name)".into());
    }
    let crate_name = args
        .id
        .rsplit('.')
        .next()
        .ok_or_else(|| "plugin id: нужен reverse-DNS (com.publisher.name)".to_string())?;
    let dir = args
        .dir
        .unwrap_or_else(|| PathBuf::from(crate_name));
    if dir.exists() {
        let empty = fs::read_dir(&dir)
            .map_err(|err| format!("{}: {err}", dir.display()))?
            .next()
            .is_none();
        if !empty {
            return Err(format!("{} уже существует", dir.display()));
        }
    } else {
        fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    fs::create_dir_all(dir.join("src")).map_err(|err| format!("src: {err}"))?;

    let name = args.name.unwrap_or_else(|| crate_name.to_string());
    let author = args.author.unwrap_or_else(|| "author".to_string());
    let sdk_path = sdk_guest_path(&dir)?;
    let cargo = cargo_toml(crate_name, &sdk_path, args.role.feature());
    fs::write(dir.join("Cargo.toml"), cargo).map_err(|err| format!("Cargo.toml: {err}"))?;
    fs::write(dir.join("src/lib.rs"), lib_rs(args.role, args.mode))
        .map_err(|err| format!("src/lib.rs: {err}"))?;
    fs::write(
        dir.join("manifest"),
        manifest_json(&args.id, &name, &author, args.role, crate_name, args.mode),
    )
    .map_err(|err| format!("manifest: {err}"))?;
    fs::write(dir.join(".gitignore"), "/target\n").map_err(|err| format!(".gitignore: {err}"))?;
    if args.role == Role::Widget {
        write_web_assets(&dir, "web")?;
    }
    if args.role == Role::Embedder {
        write_embed_assets(&dir)?;
    }
    if args.role == Role::Player {
        write_player_sfx(&dir)?;
    }
    if args.role == Role::Bridge {
        write_bridge_settings(&dir)?;
    }
    if args.role == Role::Rates {
        write_rates_settings(&dir)?;
    }
    if args.role == Role::Alerter {
        write_web_assets(&dir, "web")?;
    }
    if args.role == Role::Panel {
        match args.mode.unwrap_or(PanelMode::Native) {
            PanelMode::Native => write_panel_json(&dir)?,
            PanelMode::Web => write_web_assets(&dir, "panel")?,
        }
    }
    Ok(dir)
}

fn sdk_guest_path(plugin_dir: &Path) -> Result<String, String> {
    let guest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../guest");
    let guest = strip_verbatim(
        guest
            .canonicalize()
            .map_err(|err| format!("sdk/guest: {err}"))?,
    );
    fs::create_dir_all(plugin_dir).map_err(|err| format!("{}: {err}", plugin_dir.display()))?;
    let plugin = if plugin_dir.is_absolute() {
        plugin_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| format!("cwd: {err}"))?
            .join(plugin_dir)
    };
    let plugin = strip_verbatim(
        plugin
            .canonicalize()
            .map_err(|err| format!("{}: {err}", plugin.display()))?,
    );
    let rel = pathdiff::diff_paths(&guest, &plugin).unwrap_or(guest);
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

fn cargo_toml(crate_name: &str, sdk_path: &str, feature: &str) -> String {
    format!(
        "[package]
name = \"{crate_name}\"
version = \"0.1.0\"
edition = \"2021\"

[lib]
crate-type = [\"cdylib\"]

[dependencies]
modus-sdk = {{ path = \"{sdk_path}\", default-features = false, features = [\"{feature}\"] }}

[profile.release]
lto = true
opt-level = \"s\"
panic = \"abort\"
"
    )
}

fn lib_rs(role: Role, mode: Option<PanelMode>) -> String {
    match role {
        Role::Consumer => r#"use modus_sdk::log::{self, Level};
use modus_sdk::types::{Fragment, Payload};
use modus_sdk::wait::{self, Event, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => log_bus(&event),
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn log_bus(event: &Event) {
    log::log(
        Level::Info,
        &format!(
            "bus {} {}:{} hide={} skip={} hi={} mask={} {}",
            payload_kind(&event.payload),
            event.source.plugin_id,
            event.source.channel,
            event.flags.hide_chat,
            event.flags.skip_alert,
            event.flags.highlight,
            event.flags.mask.as_deref().unwrap_or("-"),
            payload_text(&event.payload)
        ),
    );
}

fn payload_kind(payload: &Payload) -> &'static str {
    match payload {
        Payload::Message(_) => "message",
        Payload::Donation(_) => "donation",
        Payload::Sub(_) => "sub",
        Payload::Follow(_) => "follow",
        Payload::Raid(_) => "raid",
        Payload::ViewerCount(_) => "viewer_count",
        Payload::Reward(_) => "reward",
        Payload::Moderation(_) => "moderation",
        Payload::System(_) => "system",
        Payload::Custom(_) => "custom",
    }
}

fn payload_text(payload: &Payload) -> String {
    match payload {
        Payload::Message(msg) => fragments_text(&msg.fragments),
        Payload::Donation(don) => {
            let text = fragments_text(&don.fragments);
            if text.is_empty() {
                format!("{} {}", don.money.amount, don.money.currency)
            } else {
                format!("{} {} {text}", don.money.amount, don.money.currency)
            }
        }
        Payload::Sub(sub) => fragments_text(&sub.fragments),
        Payload::Follow(follow) => follow.display_name.clone(),
        Payload::Raid(raid) => raid.from_display_name.clone(),
        Payload::ViewerCount(item) => item.count.to_string(),
        Payload::Reward(item) => {
            let text = fragments_text(&item.fragments);
            if text.is_empty() {
                format!("{} {}", item.cost, item.title)
            } else {
                format!("{} {} {text}", item.cost, item.title)
            }
        }
        Payload::Moderation(item) => item.target_display_name.clone(),
        Payload::System(ev) => format!("{:?}", ev.code),
        Payload::Custom(custom) => custom.kind.clone(),
    }
}

fn fragments_text(fragments: &[Fragment]) -> String {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Reader => r#"use modus_sdk::history_read;
use modus_sdk::log::{self, Level};
use modus_sdk::types::{Fragment, Payload};
use modus_sdk::wait::{self, Event, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
        dump_history();
    }

    fn run() {
        dump_history();
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => log_bus("bus", &event),
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn dump_history() {
    match history_read::read(None, 50) {
        Ok(page) => {
            for event in page.events {
                log_bus("history", &event);
            }
        }
        Err(err) => log::log(Level::Warn, &err),
    }
}

fn log_bus(tag: &str, event: &Event) {
    log::log(
        Level::Info,
        &format!(
            "{tag} {} {}:{} hide={} skip={} hi={} mask={} {}",
            payload_kind(&event.payload),
            event.source.plugin_id,
            event.source.channel,
            event.flags.hide_chat,
            event.flags.skip_alert,
            event.flags.highlight,
            event.flags.mask.as_deref().unwrap_or("-"),
            payload_text(&event.payload)
        ),
    );
}

fn payload_kind(payload: &Payload) -> &'static str {
    match payload {
        Payload::Message(_) => "message",
        Payload::Donation(_) => "donation",
        Payload::Sub(_) => "sub",
        Payload::Follow(_) => "follow",
        Payload::Raid(_) => "raid",
        Payload::ViewerCount(_) => "viewer_count",
        Payload::Reward(_) => "reward",
        Payload::Moderation(_) => "moderation",
        Payload::System(_) => "system",
        Payload::Custom(_) => "custom",
    }
}

fn payload_text(payload: &Payload) -> String {
    match payload {
        Payload::Message(msg) => fragments_text(&msg.fragments),
        Payload::Donation(don) => {
            let text = fragments_text(&don.fragments);
            if text.is_empty() {
                format!("{} {}", don.money.amount, don.money.currency)
            } else {
                format!("{} {} {text}", don.money.amount, don.money.currency)
            }
        }
        Payload::Sub(sub) => fragments_text(&sub.fragments),
        Payload::Follow(follow) => follow.display_name.clone(),
        Payload::Raid(raid) => raid.from_display_name.clone(),
        Payload::ViewerCount(item) => item.count.to_string(),
        Payload::Reward(item) => {
            let text = fragments_text(&item.fragments);
            if text.is_empty() {
                format!("{} {}", item.cost, item.title)
            } else {
                format!("{} {} {text}", item.cost, item.title)
            }
        }
        Payload::Moderation(item) => item.target_display_name.clone(),
        Payload::System(ev) => format!("{:?}", ev.code),
        Payload::Custom(custom) => custom.kind.clone(),
    }
}

fn fragments_text(fragments: &[Fragment]) -> String {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Player => r#"use modus_sdk::log::{self, Level};
use modus_sdk::media_audio::{self, Spec};
use modus_sdk::media_cache;
use modus_sdk::types::Payload;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;
use std::collections::HashMap;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
    }

    fn run() {
        let mut pending: HashMap<String, Option<String>> = HashMap::new();
        loop {
            match wait::wait() {
                Ready::Stop => {
                    for key in pending.into_values().flatten() {
                        let _ = media_cache::release(&key);
                    }
                    return;
                }
                Ready::Bus(event) => {
                    let play = matches!(
                        &event.payload,
                        Payload::Donation(_) | Payload::Sub(_)
                    );
                    if !play {
                        continue;
                    }
                    let audio_key = audio_key_from_opaque(event.opaque.as_deref());
                    let spec = match audio_key.as_ref() {
                        Some(key) => Spec::Url(key.clone()),
                        None => Spec::Asset("sfx.mp3".into()),
                    };
                    match media_audio::play(&spec) {
                        Ok(id) => {
                            pending.insert(id, audio_key);
                        }
                        Err(err) => log::log(Level::Warn, &err),
                    }
                }
                Ready::MediaEnded(id) => {
                    if let Some(Some(key)) = pending.remove(&id) {
                        let _ = media_cache::release(&key);
                    }
                }
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn audio_key_from_opaque(opaque: Option<&str>) -> Option<String> {
    let raw = opaque?;
    let marker = "\"audio_key\"";
    let idx = raw.find(marker)?;
    let after = &raw[idx + marker.len()..];
    let quote = after.find('"')?;
    let rest = &after[quote + 1..];
    let end = rest.find('"')?;
    let key = rest[..end].trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Bridge => r#"use modus_sdk::bridge;
use modus_sdk::log::{self, Level};
use modus_sdk::settings;
use modus_sdk::types::Payload;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
        let _ = settings::set_label("status", "ожидание событий");
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Settings => {
                    let _ = settings::set_label("status", "настройки обновлены");
                }
                Ready::Bus(event) => {
                    let scene = match &event.payload {
                        Payload::Follow(_) => follow_scene(),
                        Payload::Custom(c) if c.kind == "obs.set-scene" => {
                            scene_from_fields(&c.fields)
                        }
                        _ => None,
                    };
                    let Some(scene) = scene else {
                        continue;
                    };
                    let payload = format!("{{\"sceneName\":\"{}\"}}", escape_json(&scene));
                    match bridge::invoke("obs", "SetCurrentProgramScene", payload.as_bytes()) {
                        Ok(_) => {
                            log::log(Level::Info, &format!("scene {scene}"));
                            let _ = settings::set_label("status", &format!("сцена: {scene}"));
                        }
                        Err(err) => {
                            log::log(Level::Warn, &err);
                            let _ = settings::set_label("status", &err);
                        }
                    }
                }
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn follow_scene() -> Option<String> {
    let scene = settings::get("follow_scene").unwrap_or_default();
    let scene = scene.trim();
    if scene.is_empty() {
        None
    } else {
        Some(scene.to_string())
    }
}

fn scene_from_fields(fields: &[(String, String)]) -> Option<String> {
    fields
        .iter()
        .find(|(key, _)| key == "scene")
        .map(|(_, value)| value.clone())
        .filter(|value| !value.trim().is_empty())
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Emitter => r#"use modus_sdk::log::{self, Level};
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Act(req) => {
                    modus_sdk::chat_complete::complete(&req.id, Err("нет соединения"));
                }
                Ready::Bus(_)
                | Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Connector => r#"use modus_sdk::auth_token;
use modus_sdk::bus_emit;
use modus_sdk::log::{self, Level};
use modus_sdk::net_ws;
use modus_sdk::next_backoff_ms;
use modus_sdk::wait::{self, Ready};
use modus_sdk::wait_backoff;
use modus_sdk::{Guest, HostError, BACKOFF_START_MS};

const WS_URL: &str = "wss://example.com/";

struct Plugin;

enum Outcome {
    Stopped,
    Retry,
}

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
    }

    fn run() {
        let accounts = auth_token::list_accounts();
        if accounts.is_empty() {
            log::log(Level::Info, "нет аккаунта");
            loop {
                if matches!(wait::wait(), Ready::Stop) {
                    return;
                }
            }
        }
        run_account(&accounts[0]);
    }

    fn shutdown() {}
}

fn run_account(account_id: &str) {
    let mut backoff = BACKOFF_START_MS;
    loop {
        match run_session(account_id) {
            Outcome::Stopped => {
                log::log(Level::Info, "остановлен");
                return;
            }
            Outcome::Retry => {
                if wait_backoff(backoff) {
                    log::log(Level::Info, "остановлен");
                    return;
                }
                backoff = next_backoff_ms(backoff);
            }
        }
    }
}

fn run_session(account_id: &str) -> Outcome {
    if let Err(err) = auth_token::token(account_id) {
        return fail(&err);
    }
    let handle = match net_ws::connect(WS_URL) {
        Ok(handle) => handle,
        Err(err) => return fail(&err),
    };
    loop {
        match wait::wait() {
            Ready::Stop => {
                let _ = net_ws::close(handle);
                return Outcome::Stopped;
            }
            Ready::WsClosed(_) => {
                let _ = net_ws::close(handle);
                return fail("ws закрыт");
            }
            Ready::WsText(frame) => {
                let payload = modus_sdk::text_message("dev", "dev", frame.text, None, None);
                if let Err(err) = bus_emit::emit("example", &payload, None) {
                    if HostError::classify(&err).is_stop() {
                        let _ = net_ws::close(handle);
                        return Outcome::Stopped;
                    }
                    log::log(Level::Warn, &err);
                }
            }
            Ready::Act(req) => {
                modus_sdk::chat_complete::complete(&req.id, Err("нет соединения"));
            }
            Ready::Timer | Ready::Bus(_) | Ready::Settings | Ready::Resume | Ready::Ui(_) | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
        }
    }
}

fn fail(err: &str) -> Outcome {
    let classified = HostError::classify(err);
    if classified == HostError::Stopped {
        return Outcome::Stopped;
    }
    log::log(Level::Warn, err);
    if classified.is_stop() {
        Outcome::Stopped
    } else {
        Outcome::Retry
    }
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Provider => r#"use modus_sdk::catalog;
use modus_sdk::log::{self, Level};
use modus_sdk::media_cache;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        wait::subscribe();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => {
                    let channel = event.source.channel;
                    if channel.is_empty() {
                        continue;
                    }
                    let url = "https://example.com/emote.png";
                    let key = match media_cache::ensure(url) {
                        Ok(key) => key,
                        Err(err) => {
                            log::log(Level::Warn, &err);
                            continue;
                        }
                    };
                    let platform = event.source.platform;
                    let body = if platform.is_empty() {
                        format!(
                            "{{\"channel\":\"{channel}\",\"emotes\":[{{\"name\":\"Kappa\",\"id\":\"1\",\"key\":\"{key}\",\"zeroWidth\":false}}]}}"
                        )
                    } else {
                        format!(
                            "{{\"channel\":\"{channel}\",\"platforms\":[\"{platform}\"],\"emotes\":[{{\"name\":\"Kappa\",\"id\":\"1\",\"key\":\"{key}\",\"zeroWidth\":false}}]}}"
                        )
                    };
                    if let Err(err) = catalog::publish("emotes", body.as_bytes()) {
                        log::log(Level::Warn, &err);
                    }
                }
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Widget => r#"use modus_sdk::log::{self, Level};
use modus_sdk::ui_slot;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

fn post_n(n: u32) {
    let body = format!("{{\"n\":{n}}}");
    if let Err(err) = ui_slot::post(body.as_bytes()) {
        log::log(Level::Warn, &err);
    }
}

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        post_n(0);
    }

    fn run() {
        let mut n = 0u32;
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Ui(_) => {
                    n = n.saturating_add(1);
                    post_n(n);
                }
                Ready::Bus(_)
                | Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Embedder => r#"use modus_sdk::log::{self, Level};
use modus_sdk::media_embed;
use modus_sdk::ui_slot;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

const EMBED_URL: &str = "https://www.youtube.com/embed/dQw4w9WgXcQ?autoplay=1&enablejsapi=1&controls=0&rel=0&iv_load_policy=3&fs=0&disablekb=1&modestbranding=1";

struct Plugin;

fn post_embed() {
    if !media_embed::allowed(EMBED_URL) {
        log::log(Level::Warn, "embed url not allowed");
        return;
    }
    let body = format!("{{\"embedUrl\":\"{EMBED_URL}\"}}");
    if let Err(err) = ui_slot::post(body.as_bytes()) {
        log::log(Level::Warn, &err);
    }
}

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "embedder init");
        wait::subscribe();
        post_embed();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Settings | Ready::Resume => post_embed(),
                Ready::Bus(_)
                | Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Rates => r#"use modus_sdk::log::{self, Level};
use modus_sdk::net_http;
use modus_sdk::rates_publish::{self, Rate};
use modus_sdk::settings;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

const HOUR_MS: u64 = 3_600_000;
const NONE: &[(String, String)] = &[];

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "rates init");
        wait::subscribe();
        let _ = settings::set_label("status", "старт");
    }

    fn run() {
        loop {
            refresh();
            let hours = settings::get("interval_hours")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(6.0)
                .clamp(1.0, 168.0);
            let ms = ((hours as u64).saturating_mul(HOUR_MS)).max(60_000) as u32;
            wait::set_timer(ms);
            match wait::wait() {
                Ready::Stop => return,
                Ready::Timer | Ready::Settings | Ready::Resume => {}
                Ready::Bus(_)
                | Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Act(_)
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn refresh() {
    let base = settings::get("base")
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "RUB".into());
    let url = format!("https://open.er-api.com/v6/latest/{base}");
    match net_http::fetch("GET", &url, NONE, &[]) {
        Ok(resp) if (200..300).contains(&resp.status) => match parse_rates(&resp.body, &base) {
            Ok(rates) if !rates.is_empty() => match rates_publish::publish(&rates) {
                Ok(()) => {
                    let msg = format!("ok {} pairs", rates.len());
                    let _ = settings::set_label("status", &msg);
                    log::log(Level::Info, &msg);
                }
                Err(err) => {
                    let _ = settings::set_label("status", &err);
                    log::log(Level::Warn, &err);
                }
            },
            Ok(_) => {
                let _ = settings::set_label("status", "пустой снимок");
            }
            Err(err) => {
                let _ = settings::set_label("status", &err);
                log::log(Level::Warn, &err);
            }
        },
        Ok(resp) => {
            let msg = format!("http {}", resp.status);
            let _ = settings::set_label("status", &msg);
            log::log(Level::Warn, &msg);
        }
        Err(err) => {
            let _ = settings::set_label("status", &err);
            log::log(Level::Warn, &err);
        }
    }
}

fn parse_rates(body: &[u8], base: &str) -> Result<Vec<Rate>, String> {
    let text = std::str::from_utf8(body).map_err(|_| "utf8".to_string())?;
    let mut rates = Vec::new();
    let Some(idx) = text.find("\"rates\"") else {
        return Err("нет rates".into());
    };
    let rest = &text[idx..];
    let Some(start) = rest.find('{') else {
        return Err("битый json".into());
    };
    let slice = &rest[start + 1..];
    for part in slice.split(',') {
        let part = part.trim().trim_end_matches('}');
        let mut kv = part.splitn(2, ':');
        let Some(key) = kv.next() else { continue };
        let Some(val) = kv.next() else { continue };
        let code = key.trim().trim_matches('"').to_ascii_uppercase();
        if code.len() != 3 || code == base {
            continue;
        }
        let Ok(value) = val.trim().parse::<f64>() else {
            continue;
        };
        if !value.is_finite() || value <= 0.0 {
            continue;
        }
        // API: 1 base = value * code → invert for from=code to=base
        rates.push(Rate {
            from: code,
            to: base.to_string(),
            value: 1.0 / value,
        });
    }
    Ok(rates)
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Alerter => r##"use modus_sdk::alert_enqueue::{self, Job, Priority};
use modus_sdk::history_read;
use modus_sdk::log::{self, Level};
use modus_sdk::types::{Fragment, Payload};
use modus_sdk::ui_slot;
use modus_sdk::wait::{self, AlertCommand, Event, Ready};
use modus_sdk::Guest;

struct Plugin;

fn hide() {
    let _ = ui_slot::post(br#"{"op":"hide"}"#);
}

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "alerter init");
        wait::subscribe();
        hide();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => enqueue_bus(&event),
                Ready::AlertPlay(cmd) => on_play(&cmd),
                Ready::AlertStop(cmd) => on_stop(&cmd),
                Ready::Resume => {
                    hide();
                    recover();
                }
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Ui(_)
                | Ready::MediaEnded(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn on_play(cmd: &AlertCommand) {
    let body = format!(
        "{{\"op\":\"show\",\"jobId\":\"{}\",\"eventId\":\"{}\",\"durationMs\":{}}}",
        cmd.job_id, cmd.event_id, cmd.duration_ms
    );
    let _ = ui_slot::post(body.as_bytes());
}

fn on_stop(cmd: &AlertCommand) {
    hide();
    let _ = alert_enqueue::complete(&cmd.job_id, Ok(()));
}

fn recover() {
    let Ok(page) = history_read::read(None, 50) else {
        return;
    };
    for event in &page.events {
        if page.alert_shown.iter().any(|id| id == &event.id) {
            continue;
        }
        if matches!(
            event.payload,
            Payload::Donation(_) | Payload::Sub(_) | Payload::Raid(_) | Payload::Reward(_)
        ) {
            enqueue_bus(event);
        }
    }
}

fn enqueue_bus(event: &Event) {
    if event.flags.skip_alert {
        return;
    }
    let Some((priority, duration_ms, title, body)) = job_from_payload(&event.payload) else {
        return;
    };
    if let Err(err) = alert_enqueue::enqueue(&Job {
        event_id: event.id.clone(),
        priority,
        duration_ms,
        title,
        body,
    }) {
        log::log(Level::Warn, &err);
    }
}

fn job_from_payload(payload: &Payload) -> Option<(Priority, u32, String, String)> {
    match payload {
        Payload::Donation(don) => Some((
            Priority::Donation,
            10_000,
            format!("{} · {} {}", don.display_name, don.money.amount, don.money.currency),
            fragments_text(&don.fragments),
        )),
        Payload::Sub(sub) => Some((
            Priority::Sub,
            6_000,
            format!("{} · sub ×{}", sub.display_name, sub.months),
            fragments_text(&sub.fragments),
        )),
        Payload::Follow(follow) => Some((
            Priority::Follow,
            4_000,
            format!("{} · follow", follow.display_name),
            String::new(),
        )),
        Payload::Raid(raid) => Some((
            Priority::Raid,
            6_000,
            format!("{} · raid ×{}", raid.from_display_name, raid.viewers),
            String::new(),
        )),
        Payload::Reward(reward) => Some((
            Priority::Reward,
            5_000,
            format!("{} · {}", reward.display_name, reward.title),
            fragments_text(&reward.fragments),
        )),
        _ => None,
    }
}

fn fragments_text(fragments: &[Fragment]) -> String {
    fragments
        .iter()
        .map(|fragment| match fragment {
            Fragment::Text(text) => text.clone(),
            Fragment::Emote(emote) => emote.alt.clone(),
            Fragment::Mention(mention) => format!("@{}", mention.display_name),
            Fragment::Url(href) => href.clone(),
        })
        .collect::<Vec<_>>()
        .join("")
}

modus_sdk::export!(Plugin);
"##
        .into(),
        Role::Commander => r#"use modus_sdk::chat_act::{self, ActJob};
use modus_sdk::log::{self, Level};
use modus_sdk::types::{Fragment, Payload};
use modus_sdk::wait::{self, ActKind, Event, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "commander init");
        wait::subscribe();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => on_bus(&event),
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::Ui(_)
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

fn on_bus(event: &Event) {
    let Payload::Message(msg) = &event.payload else {
        return;
    };
    let text = fragments_text(&msg.fragments);
    let Some(job) = parse_command(&event.source.platform, &event.source.channel, &text) else {
        return;
    };
    if let Err(err) = chat_act::act(&job) {
        log::log(Level::Warn, &err);
    }
}

fn parse_command(platform: &str, channel: &str, text: &str) -> Option<ActJob> {
    let text = text.trim();
    let mut parts = text.splitn(2, char::is_whitespace);
    let cmd = parts.next()?.to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim();
    match cmd.as_str() {
        "!say" => {
            if rest.is_empty() {
                return None;
            }
            Some(ActJob {
                platform: platform.to_string(),
                channel: channel.to_string(),
                kind: ActKind::Send,
                text: Some(rest.to_string()),
                message_id: None,
                target_user_id: None,
                duration_sec: None,
            })
        }
        _ => None,
    }
}

fn fragments_text(fragments: &[Fragment]) -> String {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Store => r#"use modus_sdk::log::{self, Level};
use modus_sdk::storage_kv;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "store init");
        wait::subscribe();
        let n = match storage_kv::get("boots") {
            Ok(Some(value)) => value.parse::<u32>().unwrap_or(0),
            Ok(None) => 0,
            Err(_) => 0,
        };
        let next = n.saturating_add(1);
        let _ = storage_kv::set("boots", &next.to_string());
        log::log(Level::Info, &format!("boots {next}"));
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                _ => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
        .into(),
        Role::Panel => match mode.unwrap_or(PanelMode::Native) {
            PanelMode::Web => lib_rs(Role::Widget, None),
            PanelMode::Native => r#"use modus_sdk::log::{self, Level};
use modus_sdk::types::Payload;
use modus_sdk::ui_slot;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

fn json_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn post_state(items: &[String]) {
    let list = items
        .iter()
        .map(|item| format!("\"{}\"", json_escape(item)))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\"status\":{{\"text\":\"{} в очереди\"}},\"queue\":{{\"items\":[{}]}}}}",
        items.len(),
        list
    );
    if let Err(err) = ui_slot::post(body.as_bytes()) {
        log::log(Level::Warn, &err);
    }
}

fn click_id(payload: &[u8]) -> String {
    let Ok(text) = std::str::from_utf8(payload) else {
        return String::new();
    };
    let Some(rest) = text.split("\"id\"").nth(1) else {
        return String::new();
    };
    let Some(rest) = rest.split('"').nth(1) else {
        return String::new();
    };
    rest.to_string()
}

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "panel init");
        wait::subscribe();
        post_state(&[]);
    }

    fn run() {
        let mut items: Vec<String> = Vec::new();
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Bus(event) => {
                    let name = match &event.payload {
                        Payload::Message(msg) => msg.display_name.clone(),
                        Payload::Follow(follow) => follow.display_name.clone(),
                        _ => continue,
                    };
                    if items.len() < 32 {
                        items.push(name);
                        post_state(&items);
                    }
                }
                Ready::Ui(payload) => {
                    match click_id(&payload).as_str() {
                        "skip" => {
                            if !items.is_empty() {
                                items.remove(0);
                            }
                        }
                        "clear" => items.clear(),
                        _ => {}
                    }
                    post_state(&items);
                }
                Ready::WsText(_)
                | Ready::WsClosed(_)
                | Ready::Timer
                | Ready::Act(_)
                | Ready::Settings
                | Ready::Resume
                | Ready::MediaEnded(_)
                | Ready::AlertPlay(_)
                | Ready::AlertStop(_) => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#
            .into(),
        },
    }
}

fn manifest_json(
    id: &str,
    name: &str,
    author: &str,
    role: Role,
    platform: &str,
    mode: Option<PanelMode>,
) -> String {
    match role {
        Role::Consumer => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2
}}
"#
        ),
        Role::Reader => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["history.read"]
}}
"#
        ),
        Role::Player => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["media.audio", "media.cache"]
}}
"#
        ),
        Role::Bridge => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["bridge.obs"],
  "bridge_requests": ["SetCurrentProgramScene"]
}}
"#
        ),
        Role::Embedder => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["ui.slot", "media.embed"],
  "slots": ["web"],
  "embed_hosts": ["www.youtube.com"]
}}
"#
        ),
        Role::Rates => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["net.http", "rates.publish"],
  "hosts": ["open.er-api.com"]
}}
"#
        ),
        Role::Alerter => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["alert.enqueue", "ui.slot", "history.read", "rates.convert"],
  "slots": ["web"]
}}
"#
        ),
        Role::Commander => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["chat.act"]
}}
"#
        ),
        Role::Store => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["storage.kv"]
}}
"#
        ),
        Role::Emitter => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["bus.emit"],
  "platform_id": "{platform}"
}}
"#
        ),
        Role::Connector => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["auth.token", "net.http", "net.ws", "bus.emit", "media.cache"],
  "platform_id": "{platform}",
  "auth_mode": "pkce",
  "client_id": "your-public-client-id",
  "auth_url": "https://example.com/oauth/authorize",
  "token_url": "https://example.com/oauth/token",
  "hosts": ["example.com"]
}}
"#
        ),
        Role::Provider => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["net.http", "net.ws", "media.cache", "catalog.publish"],
  "hosts": ["example.com"],
  "provides": [{{"name": "emotes", "schema": "modus.emotes.v1"}}],
  "depends": [{{"platform": "twitch"}}]
}}
"#
        ),
        Role::Widget => format!(
            r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["ui.slot"],
  "slots": ["web"]
}}
"#
        ),
        Role::Panel => {
            let _ = mode;
            format!(
                r#"{{
  "id": "{id}",
  "name": "{name}",
  "version": "0.1.0",
  "author": "{author}",
  "abi": 2,
  "capabilities": ["ui.slot"],
  "slots": ["panel"]
}}
"#
            )
        }
    }
}

fn write_web_assets(dir: &Path, folder: &str) -> Result<(), String> {
    let web = dir.join("assets").join(folder);
    fs::create_dir_all(&web).map_err(|err| format!("assets/web: {err}"))?;
    fs::write(
        web.join("index.html"),
        r#"<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="referrer" content="no-referrer">
  <title>Modus</title>
  <link rel="stylesheet" href="overlay.css">
</head>
<body>
  <div id="hud"><span id="count">0</span> <button type="button" id="inc">+</button></div>
  <div id="feed"></div>
  <script src="overlay.js"></script>
</body>
</html>
"#,
    )
    .map_err(|err| format!("index.html: {err}"))?;
    fs::write(
        web.join("overlay.css"),
        "* { box-sizing: border-box; }\nbody { margin: 0; color: #fafafa; font-family: sans-serif; }\n#hud { padding: 8px 16px; }\n#feed { padding: 8px 16px; }\n",
    )
    .map_err(|err| format!("overlay.css: {err}"))?;
    fs::write(
        web.join("overlay.js"),
        r#"(function () {
  var proto = location.protocol === "https:" ? "wss:" : "ws:";
  var base = location.pathname.replace(/\/index\.html$/, "/").replace(/\/?$/, "/");
  var count = document.getElementById("count");
  var socket = null;
  function connect() {
    var ws = new WebSocket(proto + "//" + location.host + base + "ws");
    socket = ws;
    ws.onmessage = function (raw) {
      var frame;
      try { frame = JSON.parse(raw.data); } catch (err) { return; }
      if (frame.type === "plugin" && count) {
        try {
          var data = JSON.parse(frame.payload);
          if (data && typeof data.n === "number") count.textContent = String(data.n);
        } catch (err) {}
      }
    };
    ws.onclose = function () { setTimeout(connect, 1000); };
  }
  var inc = document.getElementById("inc");
  if (inc) {
    inc.onclick = function () {
      if (socket && socket.readyState === 1) {
        socket.send(JSON.stringify({ type: "plugin", payload: "{\"click\":1}" }));
      }
    };
  }
  connect();
})();
"#,
    )
    .map_err(|err| format!("overlay.js: {err}"))?;
    Ok(())
}

fn write_embed_assets(dir: &Path) -> Result<(), String> {
    let web = dir.join("assets").join("web");
    fs::create_dir_all(&web).map_err(|err| format!("assets/web: {err}"))?;
    fs::write(
        web.join("index.html"),
        r#"<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Embed</title>
  <link rel="stylesheet" href="overlay.css">
</head>
<body>
  <div id="stage"></div>
  <script src="overlay.js"></script>
</body>
</html>
"#,
    )
    .map_err(|err| format!("index.html: {err}"))?;
    fs::write(
        web.join("overlay.css"),
        "html, body { margin: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; }\n#stage { position: absolute; inset: 0; overflow: hidden; }\n#stage .stub,\n#stage iframe { display: block; width: 100%; height: 100%; border: 0; background: transparent; }\n#stage .stub { pointer-events: none; }\n",
    )
    .map_err(|err| format!("overlay.css: {err}"))?;
    fs::write(
        web.join("overlay.js"),
        r#"(function () {
  var stage = document.getElementById("stage");
  var proto = location.protocol === "https:" ? "wss:" : "ws:";
  var base = location.pathname.replace(/\/index\.html$/, "/").replace(/\/?$/, "/");
  var player = null;
  function mountStub() {
    player = null;
    if (!stage) return;
    stage.innerHTML = "";
    var stub = document.createElement("div");
    stub.className = "stub";
    stub.setAttribute("aria-hidden", "true");
    stage.appendChild(stub);
  }
  function ytPost(func) {
    if (!player || !player.contentWindow) return;
    player.contentWindow.postMessage(JSON.stringify({ event: "command", func: func, args: [] }), "*");
  }
  function setEmbed(url) {
    if (!stage) return;
    if (!url) {
      mountStub();
      return;
    }
    stage.innerHTML = "";
    player = null;
    try {
      var u = new URL(url);
      var defaults = {
        autoplay: "1",
        enablejsapi: "1",
        controls: "0",
        rel: "0",
        iv_load_policy: "3",
        fs: "0",
        disablekb: "1",
        modestbranding: "1",
      };
      Object.keys(defaults).forEach(function (key) {
        if (!u.searchParams.has(key)) u.searchParams.set(key, defaults[key]);
      });
      if (!u.searchParams.has("origin")) u.searchParams.set("origin", location.origin);
      url = u.toString();
    } catch (err) {}
    var frame = document.createElement("iframe");
    frame.src = url;
    frame.allow = "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share";
    frame.allowFullscreen = true;
    frame.referrerPolicy = "strict-origin-when-cross-origin";
    frame.addEventListener("load", function () {
      try { frame.contentWindow.postMessage(JSON.stringify({ event: "listening", id: 1 }), "*"); } catch (err) {}
    });
    stage.appendChild(frame);
    player = frame;
  }
  function applyCmd(cmd) {
    if (cmd === "play") ytPost("playVideo");
    else if (cmd === "pause") ytPost("pauseVideo");
  }
  function connect() {
    var ws = new WebSocket(proto + "//" + location.host + base + "ws");
    ws.onmessage = function (raw) {
      var frame;
      try { frame = JSON.parse(raw.data); } catch (err) { return; }
      if (frame.type !== "plugin") return;
      try {
        var data = JSON.parse(frame.payload);
        if (!data) return;
        if (typeof data.embedUrl === "string") setEmbed(data.embedUrl);
        if (typeof data.cmd === "string") applyCmd(data.cmd);
      } catch (err) {}
    };
    ws.onclose = function () { setTimeout(connect, 1000); };
  }
  mountStub();
  connect();
})();
"#,
    )
    .map_err(|err| format!("overlay.js: {err}"))?;
    Ok(())
}

fn write_player_sfx(dir: &Path) -> Result<(), String> {
    let assets = dir.join("assets");
    fs::create_dir_all(&assets).map_err(|err| format!("assets: {err}"))?;
    // Minimal ID3 + MPEG frame sync — enough for host sniff; null sink times by size.
    let mut bytes = b"ID3".to_vec();
    bytes.extend_from_slice(&[0u8; 29]);
    bytes.extend_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
    bytes.extend_from_slice(&[0u8; 64]);
    fs::write(assets.join("sfx.mp3"), bytes).map_err(|err| format!("sfx.mp3: {err}"))?;
    Ok(())
}

fn write_bridge_settings(dir: &Path) -> Result<(), String> {
    let assets = dir.join("assets");
    fs::create_dir_all(&assets).map_err(|err| format!("assets: {err}"))?;
    fs::write(
        assets.join("settings.json"),
        r#"{
  "version": 1,
  "sections": [
    {
      "id": "connection",
      "title": "Подключение",
      "fields": [
        {
          "key": "host",
          "type": "string",
          "label": "Host",
          "help": "Адрес локального софта. Сокет и framing держит Core.",
          "default": "127.0.0.1"
        },
        {
          "key": "port",
          "type": "number",
          "label": "Port",
          "default": 4455,
          "min": 1,
          "max": 65535
        },
        {
          "key": "password",
          "type": "secret",
          "label": "Пароль",
          "help": "Секрет WebSocket / API. Для OBS — Tools → WebSocket Server Settings."
        }
      ]
    },
    {
      "id": "scenes",
      "title": "Сцены",
      "fields": [
        {
          "key": "follow_scene",
          "type": "string",
          "label": "Сцена на follow",
          "help": "Имя сцены в OBS. Пусто — не переключать на follow.",
          "default": "Follow"
        },
        {
          "key": "status",
          "type": "label",
          "label": "Статус",
          "text": "ожидание событий"
        }
      ]
    }
  ]
}
"#,
    )
    .map_err(|err| format!("settings.json: {err}"))?;
    Ok(())
}

fn write_rates_settings(dir: &Path) -> Result<(), String> {
    let assets = dir.join("assets");
    fs::create_dir_all(&assets).map_err(|err| format!("assets: {err}"))?;
    fs::write(
        assets.join("settings.json"),
        r#"{
  "version": 1,
  "sections": [
    {
      "id": "main",
      "title": "Курсы",
      "fields": [
        {
          "key": "base",
          "type": "string",
          "label": "Базовая валюта",
          "help": "ISO-код, в который публикуются пары (from→base).",
          "default": "RUB",
          "max_len": 3
        },
        {
          "key": "interval_hours",
          "type": "number",
          "label": "Интервал, ч",
          "default": 6,
          "min": 1,
          "max": 168
        },
        {
          "key": "status",
          "type": "label",
          "label": "Статус",
          "text": "не запускался"
        }
      ]
    }
  ]
}
"#,
    )
    .map_err(|err| format!("settings.json: {err}"))?;
    Ok(())
}

fn write_panel_json(dir: &Path) -> Result<(), String> {
    let assets = dir.join("assets");
    fs::create_dir_all(&assets).map_err(|err| format!("assets: {err}"))?;
    fs::write(
        assets.join("panel.json"),
        r#"{
  "version": 2,
  "blocks": [
    { "id": "status", "type": "label", "text": "Очередь", "icon": "queue-list" },
    { "id": "queue", "type": "list" },
    {
      "id": "notes",
      "type": "table",
      "editable": true,
      "max_rows": 16,
      "columns": [
        { "id": "title", "label": "Заметка", "type": "string", "max_len": 64 },
        { "id": "done", "label": "Готово", "type": "bool" }
      ],
      "toolbar": [
        { "id": "add", "label": "Добавить", "icon": "plus" }
      ],
      "row_actions": [
        { "id": "delete", "label": "Удалить", "icon": "trash" }
      ]
    },
    { "id": "bar", "type": "buttons", "items": [
      { "id": "skip", "label": "Пропустить", "icon": "forward" },
      { "id": "clear", "label": "Очистить", "icon": "x-mark" }
    ]}
  ]
}
"#,
    )
    .map_err(|err| format!("panel.json: {err}"))?;
    Ok(())
}
