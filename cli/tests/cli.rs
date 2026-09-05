use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn modus() -> Command {
    Command::new(env!("CARGO_BIN_EXE_modus"))
}

/// Dogfood plugins live next to this repo when it is a product submodule (`../../plugins`).
/// Standalone clone of modus-sdk has no plugins — those tests skip.
fn product_plugin(name: &str) -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins").join(name);
    dir.join("manifest").is_file().then_some(dir)
}

fn run_ok(args: &[&str]) -> String {
    let out = modus()
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("modus: {err}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "modus {args:?} failed\n{stdout}{stderr}"
    );
    format!("{stdout}{stderr}")
}

fn run_err(args: &[&str]) -> String {
    let out = modus()
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("modus: {err}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "modus {args:?} should fail\n{stdout}{stderr}"
    );
    format!("{stdout}{stderr}")
}

fn unzip_wasm(mplug: &Path) -> Vec<u8> {
    let file = fs::File::open(mplug).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut entry = zip.by_name("module.wasm").unwrap();
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap();
    buf
}

fn component_imports(wasm: &[u8]) -> Vec<String> {
    use wasmparser::{Parser, Payload};
    let mut names = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        if let Payload::ComponentImportSection(section) = payload.unwrap() {
            for import in section {
                let name = import.unwrap().name.0.to_string();
                if name.contains(':') {
                    names.push(name);
                }
            }
        }
    }
    names
}

#[test]
fn new_consumer_pack_has_only_base_imports() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bus");
    run_ok(&[
        "new",
        "consumer",
        "--id",
        "com.example.bus",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("bus.mplug"), "{packed}");
    let mplug = dir.join("dist/bus.mplug");
    assert!(mplug.is_file());
    let wasm = unzip_wasm(&mplug);
    let imports = component_imports(&wasm);
    assert!(
        imports.iter().all(|imp| imp.starts_with("modus:abi/")),
        "{imports:?}"
    );
    assert!(
        imports.iter().all(|imp| !imp.contains("net-") && !imp.contains("wasi")),
        "{imports:?}"
    );
    run_ok(&["check", mplug.to_str().unwrap()]);
}

#[test]
fn new_reader_pack_imports_history() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hist");
    run_ok(&[
        "new",
        "reader",
        "--id",
        "com.example.hist",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("hist.mplug"), "{packed}");
    let mplug = dir.join("dist/hist.mplug");
    let wasm = unzip_wasm(&mplug);
    let imports = component_imports(&wasm);
    assert!(
        imports.iter().any(|imp| imp.contains("history-read")),
        "{imports:?}"
    );
    run_ok(&["check", mplug.to_str().unwrap()]);
}

#[test]
fn new_player_pack_imports_media_audio() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("sfx");
    run_ok(&[
        "new",
        "player",
        "--id",
        "com.example.sfx",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    assert!(dir.join("assets/sfx.mp3").exists());
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("sfx.mplug"), "{packed}");
    let mplug = dir.join("dist/sfx.mplug");
    let wasm = unzip_wasm(&mplug);
    let imports = component_imports(&wasm);
    assert!(
        imports.iter().any(|imp| imp.contains("media-audio")),
        "{imports:?}"
    );
    assert!(
        imports.iter().any(|imp| imp.contains("media-cache")),
        "{imports:?}"
    );
    run_ok(&["check", mplug.to_str().unwrap()]);
}

#[test]
fn new_bridge_pack_imports_bridge() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("obs");
    run_ok(&[
        "new",
        "bridge",
        "--id",
        "com.example.obs",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("obs.mplug"), "{packed}");
    let mplug = dir.join("dist/obs.mplug");
    let wasm = unzip_wasm(&mplug);
    let imports = component_imports(&wasm);
    assert!(
        imports.iter().any(|imp| imp.contains("bridge")),
        "{imports:?}"
    );
    run_ok(&["check", mplug.to_str().unwrap()]);
}

#[test]
fn pack_network_without_grant_soft_links() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("denied");
    run_ok(&[
        "new",
        "connector",
        "--id",
        "com.example.denied",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    fs::write(
        dir.join("src/lib.rs"),
        r#"use modus_sdk::log::{self, Level};
use modus_sdk::net_ws;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        log::log(Level::Info, "init");
        let _ = net_ws::connect("wss://example.com/");
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
"#,
    )
    .unwrap();
    fs::write(
        dir.join("manifest"),
        r#"{
  "id": "com.example.denied",
  "name": "denied",
  "version": "0.1.0",
  "author": "author",
  "abi": 2
}
"#,
    )
    .unwrap();
    let out = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(out.contains("denied.mplug"), "{out}");
    assert!(dir.join("dist/denied.mplug").exists());
    run_ok(&["check", dir.join("dist/denied.mplug").to_str().unwrap()]);
}

#[test]
fn new_connector_has_no_twitch_broker() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mine");
    run_ok(&[
        "new",
        "connector",
        "--id",
        "com.example.mine",
        "--dir",
        dir.to_str().unwrap(),
        "--lang",
        "rust",
    ]);
    let manifest = fs::read_to_string(dir.join("manifest")).unwrap();
    assert!(!manifest.contains("broker"), "{manifest}");
    assert!(!manifest.contains("twitch"), "{manifest}");
    assert!(!manifest.contains("yjutimasrvqwcynl0wk7i2zsfzruhi"), "{manifest}");
    assert!(manifest.contains("example.com"), "{manifest}");
    assert!(manifest.contains("pkce"), "{manifest}");
}

#[test]
fn new_provider_pack_imports_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("emotes");
    run_ok(&[
        "new",
        "provider",
        "--id",
        "com.example.emotes",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let manifest = fs::read_to_string(dir.join("manifest")).unwrap();
    assert!(manifest.contains("catalog.publish"), "{manifest}");
    assert!(manifest.contains("modus.emotes.v1"), "{manifest}");
    assert!(!manifest.contains("platform_id"), "{manifest}");
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("emotes.mplug"), "{packed}");
    let imports = component_imports(&unzip_wasm(&dir.join("dist/emotes.mplug")));
    assert!(
        imports.iter().any(|imp| imp.contains("catalog@2.0.0")),
        "{imports:?}"
    );
}

#[test]
fn new_widget_pack_imports_ui_slot() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hud");
    run_ok(&[
        "new",
        "widget",
        "--id",
        "com.example.hud",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let manifest = fs::read_to_string(dir.join("manifest")).unwrap();
    assert!(manifest.contains("ui.slot"), "{manifest}");
    assert!(manifest.contains("\"web\""), "{manifest}");
    assert!(dir.join("assets/web/index.html").is_file());
    let packed = run_ok(&["pack", dir.to_str().unwrap()]);
    assert!(packed.contains("hud.mplug"), "{packed}");
    let imports = component_imports(&unzip_wasm(&dir.join("dist/hud.mplug")));
    assert!(
        imports.iter().any(|imp| imp.contains("ui-slot@2.0.0")),
        "{imports:?}"
    );
}

#[test]
fn new_panel_native_and_web() {
    let tmp = tempfile::tempdir().unwrap();
    let native = tmp.path().join("queue");
    run_ok(&[
        "new",
        "panel",
        "--id",
        "com.example.queue",
        "--dir",
        native.to_str().unwrap(),
    ]);
    let manifest = fs::read_to_string(native.join("manifest")).unwrap();
    assert!(manifest.contains("ui.slot"), "{manifest}");
    assert!(manifest.contains("\"panel\""), "{manifest}");
    assert!(!manifest.contains("\"web\""), "{manifest}");
    assert!(native.join("assets/panel.json").is_file());
    assert!(!native.join("assets/panel/index.html").is_file());
    let packed = run_ok(&["pack", native.to_str().unwrap()]);
    assert!(packed.contains("queue.mplug"), "{packed}");
    let imports = component_imports(&unzip_wasm(&native.join("dist/queue.mplug")));
    assert!(
        imports.iter().any(|imp| imp.contains("ui-slot@2.0.0")),
        "{imports:?}"
    );

    let web = tmp.path().join("hud");
    run_ok(&[
        "new",
        "panel",
        "--mode",
        "web",
        "--id",
        "com.example.hud",
        "--dir",
        web.to_str().unwrap(),
    ]);
    assert!(web.join("assets/panel/index.html").is_file());
    assert!(!web.join("assets/panel.json").is_file());
    let packed = run_ok(&["pack", web.to_str().unwrap()]);
    assert!(packed.contains("hud.mplug"), "{packed}");
}

#[test]
fn new_rejects_non_rust() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("go");
    let err = run_err(&[
        "new",
        "consumer",
        "--id",
        "com.example.go",
        "--dir",
        dir.to_str().unwrap(),
        "--lang",
        "go",
    ]);
    assert!(err.contains("Rust"), "{err}");
    assert!(!dir.join("Cargo.toml").exists());
}

fn wait_dev_output(args: &[&str], needle: &str) -> String {
    let mut child = modus()
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("modus dev: {err}"));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    let tx_out = tx.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let _ = tx.send(line.clone());
            line.clear();
        }
    });
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            let _ = tx_out.send(line.clone());
            line.clear();
        }
    });
    let start = Instant::now();
    let mut acc = String::new();
    while start.elapsed() < Duration::from_secs(180) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                acc.push_str(&line);
                if acc.contains(needle) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return acc;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    panic!("modus dev exited {status}: {acc}");
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("modus {args:?} timeout waiting for {needle:?}\n{acc}");
}

#[test]
fn new_consumer_dev_injects_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bus");
    run_ok(&[
        "new",
        "consumer",
        "--id",
        "com.example.bus",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let inject = tmp.path().join("event.json");
    fs::write(
        &inject,
        r#"{"type":"message","user_id":"fixture","display_name":"fixture","fragments":[{"type":"text","text":"fixture hello"}]}"#,
    )
    .unwrap();
    let out = wait_dev_output(
        &[
            "dev",
            dir.to_str().unwrap(),
            "--inject",
            inject.to_str().unwrap(),
        ],
        "fixture hello",
    );
    assert!(out.contains("fixture hello"), "{out}");
}

#[test]
fn new_consumer_dev_default_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("seen");
    run_ok(&[
        "new",
        "consumer",
        "--id",
        "com.example.seen",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let out = wait_dev_output(&["dev", dir.to_str().unwrap()], "fixture hello");
    assert!(out.contains("fixture hello"), "{out}");
}

#[test]
fn new_connector_dev_replays_frame() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mine");
    run_ok(&[
        "new",
        "connector",
        "--id",
        "com.example.mine",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let replay = tmp.path().join("frames.replay");
    fs::write(&replay, "hello-dev\n").unwrap();
    let out = wait_dev_output(
        &[
            "dev",
            dir.to_str().unwrap(),
            "--token",
            "fake",
            "--replay",
            replay.to_str().unwrap(),
        ],
        "hello-dev",
    );
    assert!(out.contains("hello-dev"), "{out}");
}

#[test]
fn new_connector_dev_rejects_host_outside_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mine");
    run_ok(&[
        "new",
        "connector",
        "--id",
        "com.example.mine",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    fs::write(
        dir.join("src/lib.rs"),
        r#"use modus_sdk::log::{self, Level};
use modus_sdk::net_ws;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {}

    fn run() {
        if let Err(err) = net_ws::connect("wss://evil.example/") {
            log::log(Level::Warn, &err);
        }
        loop {
            if matches!(wait::wait(), Ready::Stop) {
                return;
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#,
    )
    .unwrap();
    let replay = tmp.path().join("frames.replay");
    fs::write(&replay, "ignored\n").unwrap();
    let out = wait_dev_output(
        &[
            "dev",
            dir.to_str().unwrap(),
            "--token",
            "fake",
            "--replay",
            replay.to_str().unwrap(),
        ],
        "not in manifest",
    );
    assert!(out.contains("not in manifest"), "{out}");
}

#[test]
fn twitch_dev_replays_irc() {
    let Some(twitch) = product_plugin("twitch") else {
        return;
    };
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = wait_dev_output(
        &[
            "dev",
            twitch.to_str().unwrap(),
            "--token",
            "fake",
            "--replay",
            fixtures.join("irc.replay").to_str().unwrap(),
            "--http-file",
            fixtures.join("helix.json").to_str().unwrap(),
        ],
        "hello there",
    );
    assert!(out.contains("hello there"), "{out}");
}

#[test]
fn goodgame_dev_replays_chat() {
    let Some(plugin) = product_plugin("goodgame") else {
        return;
    };
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = wait_dev_output(
        &[
            "dev",
            plugin.to_str().unwrap(),
            "--token",
            "fake",
            "--replay",
            fixtures.join("gg.replay").to_str().unwrap(),
            "--http-file",
            fixtures.join("gg.http.json").to_str().unwrap(),
        ],
        "hello gg",
    );
    assert!(out.contains("hello gg"), "{out}");
}

#[test]
fn donationalerts_dev_replays_donation() {
    let Some(plugin) = product_plugin("donationalerts") else {
        return;
    };
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = wait_dev_output(
        &[
            "dev",
            plugin.to_str().unwrap(),
            "--token",
            "fake",
            "--replay",
            fixtures.join("da.replay").to_str().unwrap(),
            "--http-file",
            fixtures.join("da.http.json").to_str().unwrap(),
        ],
        "Hello!",
    );
    assert!(out.contains("Hello!"), "{out}");
}

#[test]
fn pack_sign_and_check_verifies() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("signed-bus");
    let key = tmp.path().join("test.key");
    let trusted = tmp.path().join("test.pub.json");
    run_ok(&[
        "new",
        "consumer",
        "--id",
        "com.example.signed",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    run_ok(&[
        "keygen",
        "--out",
        key.to_str().unwrap(),
        "--key-id",
        "cli-test-key",
        "--issuer",
        "CLI Test",
    ]);
    assert!(trusted.is_file(), "trusted keys not written");
    let packed = run_ok(&[
        "pack",
        dir.to_str().unwrap(),
        "--sign",
        "--key-file",
        key.to_str().unwrap(),
    ]);
    assert!(packed.contains("signed"), "{packed}");
    let mplug = dir.join("dist/signed-bus.mplug");
    let checked = run_ok(&[
        "check",
        mplug.to_str().unwrap(),
        "--trusted-keys",
        trusted.to_str().unwrap(),
    ]);
    assert!(checked.contains("(signed)"), "{checked}");
}

#[test]
fn check_rejects_tampered_signature() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("tamper-bus");
    let key = tmp.path().join("test.key");
    let trusted = tmp.path().join("test.pub.json");
    run_ok(&[
        "new",
        "consumer",
        "--id",
        "com.example.tamper",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    run_ok(&[
        "keygen",
        "--out",
        key.to_str().unwrap(),
        "--key-id",
        "cli-tamper-key",
    ]);
    run_ok(&[
        "pack",
        dir.to_str().unwrap(),
        "--sign",
        "--key-file",
        key.to_str().unwrap(),
    ]);
    let mplug = dir.join("dist/tamper-bus.mplug");
    tamper_signature_digest(&mplug);
    let err = run_err(&[
        "check",
        mplug.to_str().unwrap(),
        "--trusted-keys",
        trusted.to_str().unwrap(),
    ]);
    assert!(err.contains("signature: Invalid"), "{err}");
}

fn tamper_signature_digest(mplug: &Path) {
    use modus_sign::{read_signature_file, SignatureFile, SIGNATURE_ENTRY};
    use std::io::{Cursor, Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    let bytes = fs::read(mplug).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut sig_entry = archive.by_name(SIGNATURE_ENTRY).unwrap();
    let mut sig_raw = Vec::new();
    sig_entry.read_to_end(&mut sig_raw).unwrap();
    let mut sig: SignatureFile = read_signature_file(&sig_raw).unwrap();
    sig.digest =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
    let mut out = Vec::new();
    {
        let cursor = Cursor::new(&mut out);
        let mut writer = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().to_string();
            if name == SIGNATURE_ENTRY {
                continue;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            writer.start_file(&name, opts).unwrap();
            writer.write_all(&data).unwrap();
        }
        let json = serde_json::to_vec(&sig).unwrap();
        writer.start_file(SIGNATURE_ENTRY, opts).unwrap();
        writer.write_all(&json).unwrap();
        writer.finish().unwrap();
    }
    fs::write(mplug, out).unwrap();
}

#[test]
fn new_store_dev_writes_kv() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("kv");
    run_ok(&[
        "new",
        "store",
        "--id",
        "com.example.kv",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let out = wait_dev_output(&["dev", dir.to_str().unwrap()], "boots 1");
    assert!(out.contains("boots 1"), "{out}");
    assert!(!out.contains("no connection"), "{out}");
}

#[test]
fn store_dev_settings_overlay() {
    let Some(store) = product_plugin("store") else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let overlay = tmp.path().join("settings.json");
    fs::write(
        &overlay,
        r#"{"note":"hello-settings","echo":true,"token":"sekret"}"#,
    )
    .unwrap();
    let out = wait_dev_output(
        &[
            "dev",
            store.to_str().unwrap(),
            "--settings",
            overlay.to_str().unwrap(),
        ],
        "secret=yes",
    );
    assert!(out.contains("note hello-settings"), "{out}");
    assert!(out.contains("secret=yes"), "{out}");
}

#[test]
fn new_alerter_dev_enqueues() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("alerts");
    run_ok(&[
        "new",
        "alerter",
        "--id",
        "com.example.alerts",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let out = wait_dev_output(&["dev", dir.to_str().unwrap()], "alert enqueue");
    assert!(out.contains("alert enqueue"), "{out}");
}

#[test]
fn new_commander_dev_acts() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("cmd");
    run_ok(&[
        "new",
        "commander",
        "--id",
        "com.example.cmd",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let inject = tmp.path().join("say.json");
    fs::write(
        &inject,
        r#"{"type":"message","user_id":"1","display_name":"mod","fragments":[{"type":"text","text":"!say hello-act"}]}"#,
    )
    .unwrap();
    let out = wait_dev_output(
        &[
            "dev",
            dir.to_str().unwrap(),
            "--inject",
            inject.to_str().unwrap(),
        ],
        "chat.act",
    );
    assert!(out.contains("chat.act"), "{out}");
    assert!(!out.contains("no connection"), "{out}");
}

#[test]
fn new_emitter_dev_act_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("emit");
    run_ok(&[
        "new",
        "emitter",
        "--id",
        "com.example.emit",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let act = tmp.path().join("act.json");
    fs::write(
        &act,
        r#"{"kind":"send","platform":"fixture","channel":"dev","text":"ping"}"#,
    )
    .unwrap();
    let out = wait_dev_output(
        &["dev", dir.to_str().unwrap(), "--act", act.to_str().unwrap()],
        "chat.complete",
    );
    assert!(out.contains("chat.complete"), "{out}");
}

#[test]
fn new_store_without_kv_grant_rejects() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("nogrant");
    run_ok(&[
        "new",
        "store",
        "--id",
        "com.example.nogrant",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    let manifest = dir.join("manifest");
    let text = fs::read_to_string(&manifest).unwrap().replace(
        r#""capabilities": ["storage.kv"]"#,
        r#""capabilities": []"#,
    );
    fs::write(&manifest, text).unwrap();
    fs::write(
        dir.join("src/lib.rs"),
        r#"use modus_sdk::log::{self, Level};
use modus_sdk::storage_kv;
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        wait::subscribe();
        if let Err(err) = storage_kv::set("boots", "1") {
            log::log(Level::Warn, &err);
        }
    }

    fn run() {
        loop {
            if matches!(wait::wait(), Ready::Stop) {
                return;
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#,
    )
    .unwrap();
    let out = wait_dev_output(
        &["dev", dir.to_str().unwrap()],
        "no grant storage.kv",
    );
    assert!(out.contains("no grant storage.kv"), "{out}");
}

#[test]
fn new_widget_dev_ui_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("ui");
    run_ok(&[
        "new",
        "widget",
        "--id",
        "com.example.ui",
        "--dir",
        dir.to_str().unwrap(),
    ]);
    fs::write(
        dir.join("src/lib.rs"),
        r#"use modus_sdk::log::{self, Level};
use modus_sdk::wait::{self, Ready};
use modus_sdk::Guest;

struct Plugin;

impl Guest for Plugin {
    fn init() {
        wait::subscribe();
    }

    fn run() {
        loop {
            match wait::wait() {
                Ready::Stop => return,
                Ready::Ui(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    log::log(Level::Info, &format!("ui {text}"));
                }
                _ => {}
            }
        }
    }

    fn shutdown() {}
}

modus_sdk::export!(Plugin);
"#,
    )
    .unwrap();
    let ui = tmp.path().join("ui.json");
    fs::write(&ui, r#""hello-ui""#).unwrap();
    let out = wait_dev_output(
        &["dev", dir.to_str().unwrap(), "--ui", ui.to_str().unwrap()],
        "ui hello-ui",
    );
    assert!(out.contains("ui hello-ui"), "{out}");
}

