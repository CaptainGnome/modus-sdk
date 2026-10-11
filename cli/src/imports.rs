use crate::manifest::Capability;
use std::collections::HashSet;

const V20: &str = "2.0.0";
const V21: &str = "2.1.0";
const MODUS_PREFIX: &str = "modus:abi/";

/// Interface names (without package prefix / version) every plugin may import.
const BASE_NAMES: &[&str] = &[
    "self-info",
    "log",
    "wait",
    "types",
    "clock",
    "random",
    "settings",
    "assets",
    "plugins",
];

/// Optional interfaces present in both 2.0.0 and 2.1.0.
const KNOWN_NAMES: &[&str] = &[
    "bus-emit",
    "auth-token",
    "net-http",
    "net-ws",
    "alert-enqueue",
    "storage-kv",
    "chat-act",
    "chat-complete",
    "media-cache",
    "catalog",
    "ui-slot",
    "history-read",
    "media-audio",
    "net-bridge",
    "media-embed",
    "rates-publish",
    "rates",
];

/// Added in 2.1.0 only.
const KNOWN_NAMES_21_ONLY: &[&str] = &["net-grpc"];

#[allow(dead_code)]
pub const MEDIA_EMBED_IMPORT: &str = "modus:abi/media-embed@2.1.0";
#[allow(dead_code)]
pub const MEDIA_EMBED_IMPORT_V20: &str = "modus:abi/media-embed@2.0.0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiPackage {
    V20,
    V21,
}

#[allow(dead_code)]
pub fn imports_media_embed(imports: &[String]) -> bool {
    imports
        .iter()
        .any(|import| import == MEDIA_EMBED_IMPORT || import == MEDIA_EMBED_IMPORT_V20)
}

/// Split `modus:abi/<name>@<version>` into `(name, version)`.
fn split_modus(import: &str) -> Option<(&str, &str)> {
    let rest = import.strip_prefix(MODUS_PREFIX)?;
    rest.split_once('@')
}

/// Which `modus:abi` package version the component targets.
pub fn detect_abi_package(imports: &[String]) -> Result<AbiPackage, String> {
    let mut has_modus = false;
    let mut has20 = false;
    let mut has21 = false;
    for import in imports {
        if !import.starts_with(MODUS_PREFIX) {
            continue;
        }
        has_modus = true;
        match split_modus(import).map(|(_, version)| version) {
            Some(V20) => has20 = true,
            Some(V21) => has21 = true,
            _ => {}
        }
    }
    if !has_modus {
        return Err("no modus:abi imports".into());
    }
    match (has20, has21) {
        (true, true) => Err("mixed ABI package versions".into()),
        (true, false) => Ok(AbiPackage::V20),
        (false, true) => Ok(AbiPackage::V21),
        (false, false) => Err("unsupported modus:abi version".into()),
    }
}

fn is_allowed(name: &str, version: &str) -> bool {
    if BASE_NAMES.contains(&name) || KNOWN_NAMES.contains(&name) {
        return version == V20 || version == V21;
    }
    KNOWN_NAMES_21_ONLY.contains(&name) && version == V21
}

/// Soft-link: known modus imports ok without grant. Rights enforced on call.
/// Accepts both `@2.0.0` (no net-grpc) and `@2.1.0` sets; mixing is checked by
/// `detect_abi_package`.
/// `granted` / `has_ui_surface` kept for call-site signature parity with Core.
pub fn validate_imports(
    imports: &[String],
    _granted: &HashSet<Capability>,
    _has_ui_surface: bool,
) -> Result<(), String> {
    let mut has_modus = false;
    for import in imports {
        if import.starts_with("wasi:") || import.starts_with("wasi-") {
            return Err(format!("forbidden import {import}"));
        }
        if let Some((name, version)) = split_modus(import) {
            has_modus = true;
            if is_allowed(name, version) {
                continue;
            }
        }
        return Err(format!("extra import {import}"));
    }
    if has_modus {
        detect_abi_package(imports)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(imports: &[String], granted: &HashSet<Capability>) {
        assert!(validate_imports(imports, granted, false).is_ok());
    }

    fn err(imports: &[String], granted: &HashSet<Capability>) {
        assert!(validate_imports(imports, granted, false).is_err());
    }

    fn versioned(names: &[&str], version: &str) -> Vec<String> {
        names
            .iter()
            .map(|name| format!("{MODUS_PREFIX}{name}@{version}"))
            .collect()
    }

    #[test]
    fn base_imports_ok() {
        for version in [V20, V21] {
            ok(&versioned(BASE_NAMES, version), &HashSet::new());
        }
    }

    #[test]
    fn wasi_rejected() {
        let imports = vec!["wasi:cli/environment@0.2.0".into()];
        err(&imports, &HashSet::new());
    }

    #[test]
    fn known_imports_ok_without_grant() {
        for version in [V20, V21] {
            for imp in versioned(KNOWN_NAMES, version) {
                ok(&[imp], &HashSet::new());
            }
        }
        for imp in versioned(KNOWN_NAMES_21_ONLY, V21) {
            ok(&[imp], &HashSet::new());
        }
    }

    #[test]
    fn net_grpc_rejected_on_v20() {
        let imports = vec!["modus:abi/net-grpc@2.0.0".into()];
        err(&imports, &HashSet::new());
    }

    #[test]
    fn ui_slot_ok_without_grant_or_surface() {
        let imports = vec!["modus:abi/ui-slot@2.1.0".into()];
        assert!(validate_imports(&imports, &HashSet::new(), false).is_ok());
        assert!(validate_imports(&imports, &HashSet::new(), true).is_ok());
    }

    #[test]
    fn abi1_import_rejected() {
        let imports = vec!["modus:abi/bus-emit@1.0.0".into()];
        let mut granted = HashSet::new();
        granted.insert(Capability::BusEmit);
        err(&imports, &granted);
    }

    #[test]
    fn media_embed_either_version() {
        assert!(imports_media_embed(&["modus:abi/media-embed@2.0.0".into()]));
        assert!(imports_media_embed(&["modus:abi/media-embed@2.1.0".into()]));
        assert!(!imports_media_embed(&["modus:abi/log@2.1.0".into()]));
    }

    #[test]
    fn detect_v20_ok() {
        let imports = versioned(BASE_NAMES, V20);
        assert_eq!(detect_abi_package(&imports), Ok(AbiPackage::V20));
    }

    #[test]
    fn detect_v21_ok() {
        let mut imports = versioned(BASE_NAMES, V21);
        imports.extend(versioned(KNOWN_NAMES_21_ONLY, V21));
        assert_eq!(detect_abi_package(&imports), Ok(AbiPackage::V21));
    }

    #[test]
    fn detect_mix_err() {
        let imports = vec![
            "modus:abi/log@2.0.0".to_string(),
            "modus:abi/wait@2.1.0".to_string(),
        ];
        assert_eq!(
            detect_abi_package(&imports),
            Err("mixed ABI package versions".into())
        );
        err(&imports, &HashSet::new());
    }

    #[test]
    fn detect_none_or_unknown_err() {
        assert!(detect_abi_package(&[]).is_err());
        assert!(detect_abi_package(&["wasi:cli/environment@0.2.0".into()]).is_err());
        assert!(detect_abi_package(&["modus:abi/log@1.0.0".into()]).is_err());
    }
}
