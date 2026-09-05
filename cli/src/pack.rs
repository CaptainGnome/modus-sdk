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

pub fn compile_component(plugin_dir: &Path) -> Result<Vec<u8>, String> {
    compile_component_profile(plugin_dir, Profile::Release)
}

pub fn compile_component_profile(plugin_dir: &Path, profile: Profile) -> Result<Vec<u8>, String> {
    if !plugin_dir.join("Cargo.toml").is_file() {
        return Err(format!("missing Cargo.toml in {}", plugin_dir.display()));
    }
    let package_name = crate_package_name(plugin_dir)?;
    cargo_build(plugin_dir, profile)?;
    let wasm_path = wasm_artifact(plugin_dir, &package_name, profile);
    let module = fs::read(&wasm_path).map_err(|err| {
        format!(
            "missing {}: {err}. Need target wasm32-unknown-unknown",
            wasm_path.display()
        )
    })?;
    encode_component(&module)
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

fn cargo_build(plugin_dir: &Path, profile: Profile) -> Result<(), String> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let target_dir = plugin_dir.join("target");
    let mut cmd = Command::new(cargo);
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(plugin_dir.join("Cargo.toml"))
        .args(["--target", "wasm32-unknown-unknown"])
        .env("CARGO_TARGET_DIR", &target_dir);
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

fn wasm_artifact(plugin_dir: &Path, package_name: &str, profile: Profile) -> PathBuf {
    let stem = package_name.replace('-', "_");
    plugin_dir.join(format!(
        "target/wasm32-unknown-unknown/{}/{stem}.wasm",
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
