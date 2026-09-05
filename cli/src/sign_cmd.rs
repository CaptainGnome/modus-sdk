use modus_sign::{sign_package_bytes, SigningKeyFile, TrustedKeys, verify_package_bytes, SignatureStatus};
use std::fs;
use std::path::Path;

pub fn keygen(out: &Path, key_id: &str, issuer: &str) -> Result<(), String> {
    let key = SigningKeyFile::generate(key_id, issuer);
    let trusted = key.public_trusted_key()?;
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
        }
    }
    let key_json = serde_json::to_string_pretty(&key).map_err(|err| err.to_string())?;
    fs::write(out, key_json).map_err(|err| format!("записать ключ: {err}"))?;
    let pub_path = out.with_extension("pub.json");
    let pub_json = serde_json::to_string_pretty(&serde_json::json!({
        "keys": [trusted],
        "revoked": []
    }))
    .map_err(|err| err.to_string())?;
    fs::write(&pub_path, pub_json).map_err(|err| format!("записать pub: {err}"))?;
    println!("key {}", out.display());
    println!("trusted {}", pub_path.display());
    Ok(())
}

pub fn sign_mplug(path: &Path, key_path: &Path) -> Result<(), String> {
    let key = SigningKeyFile::load(key_path)?;
    let bytes = fs::read(path).map_err(|err| format!("читать {}: {err}", path.display()))?;
    let signed = sign_package_bytes(&bytes, &key)?;
    fs::write(path, signed).map_err(|err| format!("записать {}: {err}", path.display()))?;
    Ok(())
}

pub fn verify_mplug(path: &Path, trusted_path: Option<&Path>) -> Result<SignatureStatus, String> {
    let bytes = fs::read(path).map_err(|err| format!("читать {}: {err}", path.display()))?;
    let trusted = load_trusted(trusted_path)?;
    let plugin_id = crate::check::check_mplug(path).ok().map(|m| m.id);
    let meta = verify_package_bytes(
        &bytes,
        &trusted,
        plugin_id.as_deref(),
    )?;
    Ok(meta.status)
}

pub fn load_trusted(path: Option<&Path>) -> Result<TrustedKeys, String> {
    if let Some(path) = path {
        return TrustedKeys::load(path);
    }
    if let Ok(path) = std::env::var("MODUS_TRUSTED_KEYS") {
        if !path.trim().is_empty() {
            return TrustedKeys::load(Path::new(path.trim()));
        }
    }
    Ok(TrustedKeys::empty())
}

pub fn resolve_sign_key(key_file: Option<&Path>) -> Result<Option<std::path::PathBuf>, String> {
    if let Some(path) = key_file {
        return Ok(Some(path.to_path_buf()));
    }
    if let Ok(path) = std::env::var("MODUS_SIGN_KEY") {
        let path = path.trim();
        if !path.is_empty() {
            return Ok(Some(PathBuf::from(path)));
        }
    }
    Ok(None)
}

use std::path::PathBuf;
