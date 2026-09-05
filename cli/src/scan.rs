use std::fs;
use std::path::Path;

pub fn has_modus_sdk(plugin_dir: &Path) -> Result<bool, String> {
    let text = fs::read_to_string(plugin_dir.join("Cargo.toml"))
        .map_err(|err| format!("Cargo.toml: {err}"))?;
    let value: toml::Value = text.parse().map_err(|err| format!("Cargo.toml: {err}"))?;
    Ok(value
        .get("dependencies")
        .and_then(|deps| deps.get("modus-sdk"))
        .is_some())
}

pub fn has_wit_bindgen_generate(plugin_dir: &Path) -> Result<bool, String> {
    scan_dir(&plugin_dir.join("src"))
}

fn scan_dir(dir: &Path) -> Result<bool, String> {
    if !dir.is_dir() {
        return Ok(false);
    }
    let entries = fs::read_dir(dir).map_err(|err| format!("src: {err}"))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("src: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            if scan_dir(&path)? {
                return Ok(true);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
        if text.contains("wit_bindgen::generate") {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn reject_dual_bindgen(plugin_dir: &Path) -> Result<(), String> {
    if has_modus_sdk(plugin_dir)? && has_wit_bindgen_generate(plugin_dir)? {
        return Err("manual WIT plus SDK — dual bindgen".into());
    }
    Ok(())
}
