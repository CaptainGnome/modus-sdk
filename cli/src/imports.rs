use crate::manifest::Capability;
use std::collections::HashSet;

const V21: &str = "2.1.0";
const V22: &str = "2.2.0";
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

/// Optional interfaces present in both 2.1.0 and 2.2.0.
const KNOWN_NAMES: &[&str] = &[
    "bus-emit",
    "auth-token",
    "net-http",
    "net-ws",
    "net-grpc",
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

/// Optional interfaces only in `@2.2.0` (not in frozen 2.1).
const V22_ONLY_NAMES: &[&str] = &["chat-mangle"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiPackage {
    V21,
    V22,
}

/// Split `modus:abi/<name>@<version>` into `(name, version)`.
fn split_modus(import: &str) -> Option<(&str, &str)> {
    let rest = import.strip_prefix(MODUS_PREFIX)?;
    rest.split_once('@')
}

/// Which `modus:abi` package version the component targets.
pub fn detect_abi_package(imports: &[String]) -> Result<AbiPackage, String> {
    let mut has_modus = false;
    let mut has21 = false;
    let mut has22 = false;
    for import in imports {
        if !import.starts_with(MODUS_PREFIX) {
            continue;
        }
        has_modus = true;
        match split_modus(import).map(|(_, version)| version) {
            Some(V21) => has21 = true,
            Some(V22) => has22 = true,
            _ => {}
        }
    }
    if !has_modus {
        return Err("no modus:abi imports".into());
    }
    match (has21, has22) {
        (true, true) => Err("mixed ABI package versions".into()),
        (true, false) => Ok(AbiPackage::V21),
        (false, true) => Ok(AbiPackage::V22),
        (false, false) => Err("unsupported modus:abi version".into()),
    }
}

fn is_allowed(name: &str, version: &str) -> bool {
    if version == V22 && V22_ONLY_NAMES.contains(&name) {
        return true;
    }
    (BASE_NAMES.contains(&name) || KNOWN_NAMES.contains(&name))
        && (version == V21 || version == V22)
}

/// Soft-link: known modus imports ok without grant. Rights enforced on call.
/// Accepts both `@2.1.0` and `@2.2.0` sets; mixing is checked by
/// `detect_abi_package`. `@2.0.0` is rejected.
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
        for version in [V21, V22] {
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
        for version in [V21, V22] {
            for imp in versioned(KNOWN_NAMES, version) {
                ok(&[imp], &HashSet::new());
            }
        }
    }

    #[test]
    fn v20_rejected() {
        let imports = vec!["modus:abi/log@2.0.0".into()];
        err(&imports, &HashSet::new());
        assert!(detect_abi_package(&imports).is_err());
    }

    #[test]
    fn ui_slot_ok_without_grant_or_surface() {
        let imports = vec!["modus:abi/ui-slot@2.2.0".into()];
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
    fn detect_v21_ok() {
        let imports = versioned(BASE_NAMES, V21);
        assert_eq!(detect_abi_package(&imports), Ok(AbiPackage::V21));
    }

    #[test]
    fn detect_v22_ok() {
        let imports = versioned(BASE_NAMES, V22);
        assert_eq!(detect_abi_package(&imports), Ok(AbiPackage::V22));
    }

    #[test]
    fn detect_mix_err() {
        let imports = vec![
            "modus:abi/log@2.1.0".to_string(),
            "modus:abi/wait@2.2.0".to_string(),
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
        assert!(detect_abi_package(&["modus:abi/log@2.0.0".into()]).is_err());
    }
}
