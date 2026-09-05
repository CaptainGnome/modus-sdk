mod act;
mod bus;
mod event;
mod history;
mod host;
mod inject;
mod kv;
mod mailbox;
mod media;
mod net;
mod settings;
mod wit_map;

use crate::check::check_plugin_dir;
use crate::hosts::HostSpec;
use crate::manifest::Capability;
use crate::pack::{compile_component_profile, Profile};
use act::load_acts;
use bus::Bus;
use history::HistoryGate;
use host::{HostData, Runtime};
use inject::load_events;
use kv::DevKv;
use mailbox::{Mail, Mailbox};
use media::DevCache;
use net::{load_http_fixtures, load_replay, DevNet};
use settings::DevSettings;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimitsBuilder, UpdateDeadline};

const INIT_EPOCH_TICKS: u64 = 50;
const RUN_EPOCH_TICKS: u64 = 10_000;
const STOP_JOIN: Duration = Duration::from_secs(5);

pub struct DevArgs {
    pub path: Option<PathBuf>,
    pub inject: Option<PathBuf>,
    pub token: Option<String>,
    pub token_file: Option<PathBuf>,
    pub account: Option<String>,
    pub replay: Option<PathBuf>,
    pub http_file: Option<PathBuf>,
    pub ui: Option<PathBuf>,
    pub settings: Option<PathBuf>,
    pub act: Option<PathBuf>,
}

pub fn run(args: DevArgs) -> Result<(), String> {
    let plugin_dir = args.path.unwrap_or_else(|| PathBuf::from("."));
    if plugin_dir.is_file() {
        return Err("dev: нужен каталог crate, не файл".into());
    }
    let access_token = load_token(args.token, args.token_file)?;
    let auth_account = args
        .account
        .unwrap_or_else(|| "dev".into())
        .trim()
        .to_string();
    if auth_account.is_empty() {
        return Err("dev: пустой --account".into());
    }
    let replay = match args.replay {
        Some(path) => Some(load_replay(&path)?),
        None => None,
    };
    let http_fixtures = match args.http_file {
        Some(path) => load_http_fixtures(&path)?,
        None => HashMap::new(),
    };
    let events = load_events(args.inject.as_deref())?;
    let ui_payloads = match args.ui.as_deref() {
        Some(path) => load_ui(path)?,
        None => Vec::new(),
    };
    let (dev_settings, wake_settings) =
        DevSettings::load(&plugin_dir, args.settings.as_deref())?;
    let act_requests = match args.act.as_deref() {
        Some(path) => load_acts(path, &auth_account)?,
        None => Vec::new(),
    };
    let component = compile_component_profile(&plugin_dir, Profile::Debug)?;
    let manifest = check_plugin_dir(&plugin_dir, &component)?;
    let specs: Result<Vec<_>, _> = manifest.hosts.iter().map(|h| HostSpec::parse(h)).collect();
    let specs = specs?;
    let embed_specs: Result<Vec<_>, _> = manifest
        .embed_hosts
        .iter()
        .map(|h| HostSpec::parse(h))
        .collect();
    let embed_specs = embed_specs?;

    let mut config = Config::new();
    config.wasm_component_model(true);
    config.epoch_interruption(true);
    let engine = Engine::new(&config).map_err(|err| err.to_string())?;
    let ticker = engine.clone();
    thread::Builder::new()
        .name("wasm-epoch".into())
        .spawn(move || loop {
            thread::sleep(Duration::from_millis(10));
            ticker.increment_epoch();
        })
        .map_err(|err| err.to_string())?;

    let stop = Arc::new(AtomicBool::new(false));
    let mailbox = Arc::new(Mailbox::new());
    let bus = Arc::new(Bus::new(Arc::clone(&mailbox)));
    let net = DevNet::new(
        specs.clone(),
        Arc::clone(&mailbox),
        Arc::clone(&stop),
        replay,
        http_fixtures,
    )?;
    let host = HostData {
        plugin_id: manifest.id.clone(),
        version: manifest.version.clone(),
        platform_id: manifest.platform_id.clone(),
        plugin_dir: plugin_dir.clone(),
        has_emit: manifest.grants_bus_emit(),
        has_auth: manifest.grants_auth(),
        has_http: manifest.capabilities.contains(&Capability::NetHttp),
        has_ws: manifest.capabilities.contains(&Capability::NetWs),
        has_alert: manifest.capabilities.contains(&Capability::AlertEnqueue),
        has_kv: manifest.capabilities.contains(&Capability::StorageKv),
        has_chat: manifest.capabilities.contains(&Capability::ChatAct),
        has_cache: manifest.grants_media_cache(),
        has_catalog: manifest.grants_catalog(),
        has_ui: manifest.has_ui_surface(),
        has_history: manifest.grants_history(),
        has_audio: manifest.grants_media_audio(),
        has_bridge: manifest.grants_bridge_obs(),
        has_embed: manifest.grants_media_embed(),
        has_rates: manifest.grants_rates_publish(),
        has_rates_convert: manifest.grants_rates_convert(),
        stop: Arc::clone(&stop),
        mailbox: Arc::clone(&mailbox),
        bus: Arc::clone(&bus),
        history: Arc::new(HistoryGate::new()),
        net: Arc::clone(&net),
        cache: Arc::new(DevCache::new()),
        kv: Arc::new(DevKv::new()),
        settings: Arc::new(dev_settings),
        specs,
        embed_specs,
        auth_account,
        access_token,
        limits: StoreLimitsBuilder::new().memory_size(16 << 20).build(),
    };

    let wasm_component =
        Component::from_binary(&engine, &component).map_err(|err| err.to_string())?;
    let mut store = Store::new(&engine, host);
    store.limiter(|state| &mut state.limits);
    store.set_epoch_deadline(INIT_EPOCH_TICKS);

    let mut linker = Linker::new(&engine);
    Runtime::add_to_linker::<HostData, HasSelf<_>>(&mut linker, |state| state)
        .map_err(|err| err.to_string())?;
    let plugin = Runtime::instantiate(&mut store, &wasm_component, &linker).map_err(|err| {
        let msg = err.to_string();
        if msg.contains("does not have export `run`")
            || msg.contains("matching implementation was not found")
        {
            "несовместимый ABI — пересоберите плагин".into()
        } else {
            format!("не инстанцировать: {msg}")
        }
    })?;

    eprintln!("dev {} {}", manifest.id, manifest.version);
    let _ = std::io::Write::flush(&mut std::io::stderr());
    plugin
        .modus_abi_lifecycle()
        .call_init(&mut store)
        .map_err(|err| format!("init trap: {err}"))?;
    for event in events {
        store.data().bus.emit_host(event)?;
    }
    for payload in ui_payloads {
        let _ = mailbox.sender().try_send(Mail::Ui(payload));
    }
    if wake_settings {
        let _ = mailbox.sender().try_send(Mail::Settings);
    }
    for act in act_requests {
        let _ = mailbox.sender().try_send(Mail::Act(act));
    }

    store.epoch_deadline_callback(|store| {
        if store.data().stop.load(Ordering::SeqCst) {
            Err(wasmtime::Error::msg("остановлен"))
        } else {
            Ok(UpdateDeadline::Continue(RUN_EPOCH_TICKS))
        }
    });
    store.set_epoch_deadline(RUN_EPOCH_TICKS);

    let plugin_id = manifest.id.clone();
    let thread = thread::Builder::new()
        .name(format!("plugin-{plugin_id}"))
        .spawn(move || {
            let trapped = plugin.modus_abi_lifecycle().call_run(&mut store).is_err();
            if trapped {
                eprintln!("run trap {plugin_id}");
            }
            let _ = plugin.modus_abi_lifecycle().call_shutdown(&mut store);
        })
        .map_err(|err| err.to_string())?;

    wait_for_stop(&stop, &mailbox, &net)?;
    join_plugin(thread)
}

fn load_token(
    token: Option<String>,
    token_file: Option<PathBuf>,
) -> Result<Option<String>, String> {
    match (token, token_file) {
        (Some(_), Some(_)) => Err("dev: задайте --token или --token-file".into()),
        (Some(token), None) => {
            let token = token.trim().to_string();
            if token.is_empty() {
                Err("dev: пустой токен".into())
            } else {
                Ok(Some(token))
            }
        }
        (None, Some(path)) => {
            let token = fs::read_to_string(&path).map_err(|err| format!("token-file: {err}"))?;
            let token = token.trim().to_string();
            if token.is_empty() {
                Err("dev: пустой токен".into())
            } else {
                Ok(Some(token))
            }
        }
        (None, None) => Ok(None),
    }
}

fn wait_for_stop(
    stop: &Arc<AtomicBool>,
    mailbox: &Arc<Mailbox>,
    net: &Arc<DevNet>,
) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .map_err(|err| format!("ctrlc: {err}"))?;
    let _ = rx.recv();
    stop.store(true, Ordering::SeqCst);
    mailbox.wake_stop();
    net.close_all();
    Ok(())
}

fn join_plugin(handle: thread::JoinHandle<()>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = handle.join();
        let _ = tx.send(());
    });
    if rx.recv_timeout(STOP_JOIN).is_err() {
        return Err("не остановить".into());
    }
    Ok(())
}

fn load_ui(path: &std::path::Path) -> Result<Vec<Vec<u8>>, String> {
    let raw = fs::read(path).map_err(|err| format!("ui: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|err| format!("ui JSON: {err}"))?;
    match value {
        serde_json::Value::Array(items) => items.into_iter().map(ui_value_bytes).collect(),
        other => Ok(vec![ui_value_bytes(other)?]),
    }
}

fn ui_value_bytes(value: serde_json::Value) -> Result<Vec<u8>, String> {
    match value {
        serde_json::Value::String(text) => Ok(text.into_bytes()),
        other => serde_json::to_vec(&other).map_err(|err| format!("ui: {err}")),
    }
}
