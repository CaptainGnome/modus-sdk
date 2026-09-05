use crate::manifest::Capability;
use std::collections::HashSet;

const BASE_IMPORTS: &[&str] = &[
    "modus:abi/self-info@2.0.0",
    "modus:abi/log@2.0.0",
    "modus:abi/wait@2.0.0",
    "modus:abi/types@2.0.0",
    "modus:abi/clock@2.0.0",
    "modus:abi/settings@2.0.0",
    "modus:abi/assets@2.0.0",
];

const KNOWN_IMPORTS: &[&str] = &[
    "modus:abi/bus-emit@2.0.0",
    "modus:abi/auth-token@2.0.0",
    "modus:abi/net-http@2.0.0",
    "modus:abi/net-ws@2.0.0",
    "modus:abi/alert-enqueue@2.0.0",
    "modus:abi/storage-kv@2.0.0",
    "modus:abi/chat-act@2.0.0",
    "modus:abi/chat-complete@2.0.0",
    "modus:abi/media-cache@2.0.0",
    "modus:abi/catalog@2.0.0",
    "modus:abi/ui-slot@2.0.0",
    "modus:abi/history-read@2.0.0",
    "modus:abi/media-audio@2.0.0",
    "modus:abi/net-bridge@2.0.0",
    "modus:abi/media-embed@2.0.0",
    "modus:abi/rates-publish@2.0.0",
    "modus:abi/rates@2.0.0",
];

/// Soft-link: known modus imports ok without grant. Rights enforced on call.
/// `granted` / `has_ui_surface` kept for call-site signature parity with Core.
pub fn validate_imports(
    imports: &[String],
    _granted: &HashSet<Capability>,
    _has_ui_surface: bool,
) -> Result<(), String> {
    for import in imports {
        if import.starts_with("wasi:") || import.starts_with("wasi-") {
            return Err(format!("forbidden import {import}"));
        }
        if BASE_IMPORTS.iter().any(|allowed| import == *allowed) {
            continue;
        }
        if KNOWN_IMPORTS.iter().any(|allowed| import == *allowed) {
            continue;
        }
        return Err(format!("extra import {import}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasi_rejected() {
        let imports = vec!["wasi:cli/environment@0.2.0".into()];
        assert!(validate_imports(&imports, &HashSet::new(), false).is_err());
    }

    #[test]
    fn known_imports_ok_without_grant() {
        for imp in KNOWN_IMPORTS {
            let imports = vec![(*imp).into()];
            assert!(
                validate_imports(&imports, &HashSet::new(), false).is_ok(),
                "{imp}"
            );
        }
    }

    #[test]
    fn base_imports_ok() {
        let imports = BASE_IMPORTS
            .iter()
            .map(|s| (*s).into())
            .collect::<Vec<_>>();
        assert!(validate_imports(&imports, &HashSet::new(), false).is_ok());
    }

    #[test]
    fn unknown_import_rejected() {
        let imports = vec!["modus:abi/bus-emit@1.0.0".into()];
        assert!(validate_imports(&imports, &HashSet::new(), false).is_err());
    }

    #[test]
    fn ui_slot_ok_without_grant_or_surface() {
        let imports = vec!["modus:abi/ui-slot@2.0.0".into()];
        assert!(validate_imports(&imports, &HashSet::new(), false).is_ok());
    }
}
