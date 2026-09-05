use crate::i18n::{
    is_locale_code, locale_code_from_entry, parse_catalog, validate_keys_against_en, Catalogs,
    I18N_MAX_BYTES, I18N_PREFIX,
};
use crate::imports::validate_imports;
use crate::manifest::Manifest;
use crate::panel::{PanelSchema, PANEL_JSON, PANEL_MAX_BYTES};
use crate::scan::reject_dual_bindgen;
use crate::schema::{SettingsSchema, SCHEMA_MAX_BYTES};
use crate::wasm::component_imports;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

pub const SETTINGS_ENTRY: &str = "assets/settings.json";

pub fn check_manifest_and_wasm(manifest: &Manifest, component: &[u8]) -> Result<(), String> {
    let imports = component_imports(component)?;
    let granted: HashSet<_> = manifest.capabilities.iter().cloned().collect();
    validate_imports(&imports, &granted, manifest.has_ui_surface())
}

pub fn check_settings_bytes(bytes: Option<&[u8]>) -> Result<Option<SettingsSchema>, String> {
    match bytes {
        Some(raw) => Ok(Some(SettingsSchema::parse(raw)?)),
        None => Ok(None),
    }
}

fn collect_i18n_keys(
    manifest: &Manifest,
    schema: Option<&SettingsSchema>,
    panel: Option<&PanelSchema>,
) -> Vec<String> {
    let mut keys = manifest.i18n_keys();
    if let Some(schema) = schema {
        for key in schema.i18n_keys() {
            if !keys.iter().any(|item| item == &key) {
                keys.push(key);
            }
        }
    }
    if let Some(panel) = panel {
        for key in panel.i18n_keys() {
            if !keys.iter().any(|item| item == &key) {
                keys.push(key);
            }
        }
    }
    keys
}

fn read_i18n_dir(plugin_dir: &Path) -> Result<Catalogs, String> {
    let dir = plugin_dir.join("assets").join("i18n");
    if !dir.is_dir() {
        return Ok(Catalogs::new());
    }
    let mut catalogs = Catalogs::new();
    for entry in fs::read_dir(&dir).map_err(|err| format!("i18n: {err}"))? {
        let entry = entry.map_err(|err| format!("i18n: {err}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "i18n: плохое имя файла".to_string())?;
        let code = name
            .strip_suffix(".json")
            .ok_or_else(|| format!("i18n: нужен .json ({name})"))?;
        if !is_locale_code(code) {
            return Err(format!("плохой locale i18n: {code}"));
        }
        let bytes = fs::read(&path).map_err(|err| format!("i18n: {err}"))?;
        if bytes.len() > I18N_MAX_BYTES {
            return Err("i18n слишком большой".into());
        }
        let catalog = parse_catalog(&bytes)?;
        if catalogs.insert(code.to_string(), catalog).is_some() {
            return Err(format!("дубль i18n locale: {code}"));
        }
    }
    Ok(catalogs)
}

pub fn check_plugin_dir(plugin_dir: &Path, component: &[u8]) -> Result<Manifest, String> {
    reject_dual_bindgen(plugin_dir)?;
    let manifest_bytes = fs::read(plugin_dir.join("manifest"))
        .map_err(|err| format!("manifest: {err}"))?;
    let manifest = Manifest::parse(&manifest_bytes)?;
    check_manifest_and_wasm(&manifest, component)?;
    let settings_path = plugin_dir.join("assets/settings.json");
    let schema = if settings_path.is_file() {
        let bytes = fs::read(&settings_path).map_err(|err| format!("settings.json: {err}"))?;
        if bytes.len() > SCHEMA_MAX_BYTES {
            return Err("settings.json слишком большой".into());
        }
        check_settings_bytes(Some(&bytes))?
    } else {
        None
    };
    check_platform_logo_dir(plugin_dir, &manifest)?;
    let panel = check_panel_dir(plugin_dir, &manifest)?;
    let catalogs = read_i18n_dir(plugin_dir)?;
    let keys = collect_i18n_keys(&manifest, schema.as_ref(), panel.as_ref());
    validate_keys_against_en(&keys, &catalogs)?;
    Ok(manifest)
}

pub fn check_mplug(path: &Path) -> Result<Manifest, String> {
    let file = fs::File::open(path).map_err(|err| format!("не открыть пакет: {err}"))?;
    let mut archive = ZipArchive::new(file).map_err(|err| format!("пакет не zip: {err}"))?;
    let mut manifest = None;
    let mut wasm = None;
    let mut schema = None;
    let mut panel_json = None;
    let mut has_panel_html = false;
    let mut has_web_html = false;
    let mut catalogs = Catalogs::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|err| format!("zip: {err}"))?;
        let name = entry.name().replace('\\', "/");
        validate_entry_name(&name)?;
        if entry.is_dir() {
            continue;
        }
        match name.as_str() {
            "manifest" => {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|err| format!("чтение manifest: {err}"))?;
                manifest = Some(buf);
            }
            "module.wasm" => {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|err| format!("чтение module.wasm: {err}"))?;
                wasm = Some(buf);
            }
            SETTINGS_ENTRY => {
                if entry.size() > SCHEMA_MAX_BYTES as u64 {
                    return Err("settings.json слишком большой".into());
                }
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|err| format!("чтение settings.json: {err}"))?;
                schema = Some(buf);
            }
            PANEL_JSON => {
                if entry.size() > PANEL_MAX_BYTES as u64 {
                    return Err("panel.json слишком большой".into());
                }
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|err| format!("чтение panel.json: {err}"))?;
                panel_json = Some(buf);
            }
            name if name.starts_with(I18N_PREFIX) => {
                let code = locale_code_from_entry(name)
                    .ok_or_else(|| format!("плохой путь i18n: {name}"))?;
                if !is_locale_code(code) {
                    return Err(format!("плохой locale i18n: {code}"));
                }
                if entry.size() > I18N_MAX_BYTES as u64 {
                    return Err("i18n слишком большой".into());
                }
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .map_err(|err| format!("чтение i18n: {err}"))?;
                if buf.len() > I18N_MAX_BYTES {
                    return Err("i18n слишком большой".into());
                }
                let catalog = parse_catalog(&buf)?;
                if catalogs.insert(code.to_string(), catalog).is_some() {
                    return Err(format!("дубль i18n locale: {code}"));
                }
            }
            name if name.starts_with("assets/panel/") => {
                if name == "assets/panel/index.html" || name.ends_with("/index.html") {
                    has_panel_html = true;
                }
            }
            name if name == "assets/web/index.html" => {
                has_web_html = true;
            }
            _ => {}
        }
    }
    let wasm = wasm.ok_or_else(|| "в пакете нет module.wasm".to_string())?;
    if wasm.is_empty() {
        return Err("пустой module.wasm".into());
    }
    let manifest = manifest.ok_or_else(|| "в пакете нет manifest".to_string())?;
    let parsed = Manifest::parse(&manifest)?;
    check_manifest_and_wasm(&parsed, &wasm)?;
    let schema = check_settings_bytes(schema.as_deref())?;
    let panel = check_panel_kind(&parsed, panel_json.as_deref(), has_panel_html, has_web_html)?;
    if let Some(rel) = parsed.platform_logo.as_deref() {
        let entry = format!("assets/{rel}");
        let mut file = archive
            .by_name(&entry)
            .map_err(|_| format!("нет {entry}"))?;
        if file.size() > LOGO_MAX_BYTES as u64 {
            return Err("platform_logo слишком большой".into());
        }
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|err| format!("чтение platform_logo: {err}"))?;
        if buf.len() > LOGO_MAX_BYTES {
            return Err("platform_logo слишком большой".into());
        }
    }
    let keys = collect_i18n_keys(&parsed, schema.as_ref(), panel.as_ref());
    validate_keys_against_en(&keys, &catalogs)?;
    Ok(parsed)
}

const LOGO_MAX_BYTES: usize = 128 * 1024;

fn check_platform_logo_dir(plugin_dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let Some(rel) = manifest.platform_logo.as_deref() else {
        return Ok(());
    };
    let mut path = plugin_dir.join("assets");
    for part in rel.split('/') {
        path.push(part);
    }
    let bytes = fs::read(&path).map_err(|_| format!("нет assets/{rel}"))?;
    if bytes.len() > LOGO_MAX_BYTES {
        return Err("platform_logo слишком большой".into());
    }
    Ok(())
}

fn check_panel_dir(plugin_dir: &Path, manifest: &Manifest) -> Result<Option<PanelSchema>, String> {
    let json_path = plugin_dir.join("assets/panel.json");
    let json = if json_path.is_file() {
        let bytes = fs::read(&json_path).map_err(|err| format!("panel.json: {err}"))?;
        if bytes.len() > PANEL_MAX_BYTES {
            return Err("panel.json слишком большой".into());
        }
        Some(bytes)
    } else {
        None
    };
    let has_panel_html = plugin_dir.join("assets/panel/index.html").is_file();
    let has_web_html = plugin_dir.join("assets/web/index.html").is_file();
    check_panel_kind(manifest, json.as_deref(), has_panel_html, has_web_html)
}

fn check_panel_kind(
    manifest: &Manifest,
    panel_json: Option<&[u8]>,
    has_panel_html: bool,
    has_web_html: bool,
) -> Result<Option<PanelSchema>, String> {
    if !manifest.is_panel() {
        return Ok(None);
    }
    if panel_json.is_some() && has_panel_html {
        return Err("panel: один режим native или web".into());
    }
    if let Some(bytes) = panel_json {
        return Ok(Some(PanelSchema::parse(bytes)?));
    }
    if has_panel_html {
        return Ok(None);
    }
    if manifest.is_web() && has_web_html {
        return Ok(None);
    }
    Err("нет panel.json и нет страницы panel".into())
}

fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.contains('\\') || name.contains("..") || name.starts_with('/') {
        return Err("небезопасный путь в zip".into());
    }
    if name == "manifest" || name == "module.wasm" || name == "signature" {
        return Ok(());
    }
    if name == "assets" || name == "assets/" || name.starts_with("assets/") {
        return Ok(());
    }
    Err(format!("лишний файл в пакете: {name}"))
}
