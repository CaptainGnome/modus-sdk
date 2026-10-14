use crate::check::check_plugin_dir;
use crate::wasm::encode_component;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Clone, Copy)]
pub enum Profile {
    Debug,
    Release,
}

impl Profile {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

enum ProjectKind {
    Rust,
    Js,
}

fn detect_project(plugin_dir: &Path) -> Result<ProjectKind, String> {
    let cargo = plugin_dir.join("Cargo.toml").is_file();
    let pkg = plugin_dir.join("package.json").is_file();
    match (cargo, pkg) {
        (true, false) => Ok(ProjectKind::Rust),
        (false, true) => Ok(ProjectKind::Js),
        (true, true) => Err("ambiguous: both Cargo.toml and package.json".into()),
        (false, false) => Err(format!(
            "missing Cargo.toml or package.json in {}",
            plugin_dir.display()
        )),
    }
}

pub fn compile_component(plugin_dir: &Path) -> Result<Vec<u8>, String> {
    compile_component_profile(plugin_dir, Profile::Release)
}

pub fn compile_component_profile(plugin_dir: &Path, profile: Profile) -> Result<Vec<u8>, String> {
    match detect_project(plugin_dir)? {
        ProjectKind::Rust => compile_rust_component(plugin_dir, profile),
        ProjectKind::Js => compile_js_component(plugin_dir),
    }
}

fn compile_rust_component(plugin_dir: &Path, profile: Profile) -> Result<Vec<u8>, String> {
    let plugin_dir = abs_plugin_dir(plugin_dir)?;
    let package_name = crate_package_name(&plugin_dir)?;
    let target_dir = resolve_target_dir(&plugin_dir)?;
    cargo_build(&plugin_dir, profile, &target_dir)?;
    let wasm_path = wasm_artifact(&target_dir, &package_name, profile);
    let module = fs::read(&wasm_path).map_err(|err| {
        format!(
            "missing {}: {err}. Need target wasm32-unknown-unknown",
            wasm_path.display()
        )
    })?;
    encode_component(&module)
}

/// Cargo target dir for a plugin build.
///
/// Order: existing `CARGO_TARGET_DIR` → nearest ancestor `[workspace]` → `<plugin>/target`.
fn resolve_target_dir(plugin_dir: &Path) -> Result<PathBuf, String> {
    if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return Ok(path);
        }
        let cwd = env::current_dir().map_err(|err| format!("cwd: {err}"))?;
        return Ok(cwd.join(path));
    }
    if let Some(workspace_root) = find_workspace_root(plugin_dir) {
        return Ok(workspace_root.join("target"));
    }
    Ok(plugin_dir.join("target"))
}

fn find_workspace_root(plugin_dir: &Path) -> Option<PathBuf> {
    let mut dir = plugin_dir.parent()?;
    loop {
        let cargo = dir.join("Cargo.toml");
        if cargo.is_file() {
            if let Ok(text) = fs::read_to_string(&cargo) {
                if text.contains("[workspace]") {
                    return Some(dir.to_path_buf());
                }
            }
        }
        dir = dir.parent()?;
    }
}

fn abs_plugin_dir(plugin_dir: &Path) -> Result<PathBuf, String> {
    if plugin_dir.is_absolute() {
        return Ok(plugin_dir.to_path_buf());
    }
    let cwd = env::current_dir().map_err(|err| format!("cwd: {err}"))?;
    Ok(cwd.join(plugin_dir))
}

fn compile_js_component(plugin_dir: &Path) -> Result<Vec<u8>, String> {
    let plugin_dir = abs_plugin_dir(plugin_dir)?;
    let entry = if plugin_dir.join("guest.ts").is_file() {
        plugin_dir.join("guest.ts")
    } else if plugin_dir.join("guest.js").is_file() {
        plugin_dir.join("guest.js")
    } else {
        return Err(format!(
            "missing guest.ts or guest.js in {}",
            plugin_dir.display()
        ));
    };
    npm_install(&plugin_dir)?;
    let wit = plugin_dir.join("node_modules/modus-sdk/wit");
    if !wit.is_dir() {
        return Err(format!(
            "missing {} — npm install modus-sdk (file: sibling modus-sdk-js)",
            wit.display()
        ));
    }
    let bundle_config = plugin_dir.join("scripts/rolldown.config.mjs");
    if !bundle_config.is_file() {
        return Err(format!("missing {}", bundle_config.display()));
    }
    let jco = plugin_dir.join("node_modules/@bytecodealliance/jco/dist/jco.js");
    if !jco.is_file() {
        return Err(format!(
            "missing {} — need @bytecodealliance/jco 1.37.0",
            jco.display()
        ));
    }
    let out = plugin_dir.join("target/module.wasm");
    fs::create_dir_all(plugin_dir.join("target")).map_err(|err| format!("target: {err}"))?;

    let node = env::var("NODE").unwrap_or_else(|_| "node".into());
    let status = Command::new(&node)
        .arg(&jco)
        .arg("componentize")
        .arg(&entry)
        .arg("--wit")
        .arg(&wit)
        .arg("--world-name")
        .arg("plugin")
        .arg("--disable")
        .arg("stdio")
        .arg("--disable")
        .arg("random")
        .arg("--disable")
        .arg("clocks")
        .arg("--disable")
        .arg("http")
        .arg("--disable")
        .arg("fetch-event")
        .arg("--bundle")
        .arg("--bundle-config")
        .arg(&bundle_config)
        .arg("-o")
        .arg(&out)
        .current_dir(&plugin_dir)
        .status()
        .map_err(|err| format!("node/jco: {err}"))?;
    if !status.success() {
        return Err("jco componentize failed".into());
    }
    fs::read(&out).map_err(|err| format!("read {}: {err}", out.display()))
}

fn npm_bin() -> String {
    env::var("NPM").unwrap_or_else(|_| {
        if cfg!(windows) {
            "npm.cmd".into()
        } else {
            "npm".into()
        }
    })
}

fn npm_install(plugin_dir: &Path) -> Result<(), String> {
    let status = Command::new(npm_bin())
        .arg("install")
        .current_dir(plugin_dir)
        .status()
        .map_err(|err| format!("npm: {err} (Node >= 20 required)"))?;
    if !status.success() {
        return Err("npm install failed".into());
    }
    Ok(())
}

pub fn pack(plugin_dir: &Path) -> Result<PathBuf, String> {
    if !plugin_dir.join("manifest").is_file() {
        return Err(format!("missing manifest in {}", plugin_dir.display()));
    }
    let component = compile_component(plugin_dir)?;
    check_plugin_dir(plugin_dir, &component)?;

    let plugin_name = plugin_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "bad plugin directory name".to_string())?;
    let dist = plugin_dir.join("dist");
    fs::create_dir_all(&dist).map_err(|err| format!("dist: {err}"))?;
    let out = dist.join(format!("{plugin_name}.mplug"));
    write_mplug(&out, plugin_dir, &component)?;
    Ok(out)
}

fn cargo_build(plugin_dir: &Path, profile: Profile, target_dir: &Path) -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(cargo);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(plugin_dir.join("Cargo.toml"))
        .args(["--target", "wasm32-unknown-unknown"])
        .env("CARGO_TARGET_DIR", target_dir);
    if matches!(profile, Profile::Release) {
        cmd.arg("--release");
    }
    let status = cmd.status().map_err(|err| format!("cargo: {err}"))?;
    if !status.success() {
        return Err("cargo build failed".into());
    }
    Ok(())
}

pub fn crate_package_name(plugin_dir: &Path) -> Result<String, String> {
    let text = fs::read_to_string(plugin_dir.join("Cargo.toml"))
        .map_err(|err| format!("Cargo.toml: {err}"))?;
    let value: toml::Value = text.parse().map_err(|err| format!("Cargo.toml: {err}"))?;
    value
        .get("package")
        .and_then(|pkg| pkg.get("name"))
        .and_then(|name| name.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Cargo.toml: missing package.name".to_string())
}

fn wasm_artifact(target_dir: &Path, package_name: &str, profile: Profile) -> PathBuf {
    let stem = package_name.replace('-', "_");
    target_dir.join(format!(
        "wasm32-unknown-unknown/{}/{stem}.wasm",
        profile.dir_name()
    ))
}

fn write_mplug(out: &Path, plugin_dir: &Path, component: &[u8]) -> Result<(), String> {
    let manifest = fs::read(plugin_dir.join("manifest")).map_err(|err| format!("manifest: {err}"))?;
    let file = fs::File::create(out).map_err(|err| format!("create {}: {err}", out.display()))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest", opts)
        .map_err(|err| format!("zip: {err}"))?;
    zip.write_all(&manifest)
        .map_err(|err| format!("zip: {err}"))?;
    zip.start_file("module.wasm", opts)
        .map_err(|err| format!("zip: {err}"))?;
    zip.write_all(component)
        .map_err(|err| format!("zip: {err}"))?;
    add_assets(&mut zip, opts, &plugin_dir.join("assets"))?;
    zip.finish().map_err(|err| format!("zip: {err}"))?;
    Ok(())
}

fn add_assets(
    zip: &mut ZipWriter<fs::File>,
    opts: SimpleFileOptions,
    assets: &Path,
) -> Result<(), String> {
    if !assets.is_dir() {
        return Ok(());
    }
    add_assets_dir(zip, opts, assets, Path::new("assets"))
}

fn add_assets_dir(
    zip: &mut ZipWriter<fs::File>,
    opts: SimpleFileOptions,
    dir: &Path,
    prefix: &Path,
) -> Result<(), String> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|err| format!("assets: {err}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("assets: {err}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "." || name == ".." || name.contains('\\') || name.contains("..") {
            return Err(format!("bad path in assets: {name}"));
        }
        let rel = prefix.join(&*name);
        let rel_name = rel.to_string_lossy().replace('\\', "/");
        let path = entry.path();
        if path.is_dir() {
            add_assets_dir(zip, opts, &path, &rel)?;
        } else {
            let bytes = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
            zip.start_file(&rel_name, opts)
                .map_err(|err| format!("zip {rel_name}: {err}"))?;
            zip.write_all(&bytes)
                .map_err(|err| format!("write {rel_name}: {err}"))?;
        }
    }
    Ok(())
}
