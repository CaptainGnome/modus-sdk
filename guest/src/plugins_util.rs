//! Helpers over WIT `plugins.list`.

use crate::plugins::Entry;

/// Enabled platforms that declare `kind` in `emits`.
pub fn platforms_with_kind(entries: &[Entry], kind: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in entries {
        if !entry.enabled {
            continue;
        }
        let Some(platform) = entry.platform_id.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if entry.emits.iter().any(|k| k == kind) && !out.iter().any(|p| p == platform) {
            out.push(platform.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        id: &str,
        enabled: bool,
        platform: Option<&str>,
        emits: &[&str],
    ) -> Entry {
        Entry {
            plugin_id: id.into(),
            name: id.into(),
            enabled,
            platform_id: platform.map(str::to_string),
            emits: emits.iter().map(|s| (*s).to_string()).collect(),
            emits_custom: Vec::new(),
        }
    }

    #[test]
    fn filters_enabled_platforms_with_kind() {
        let entries = vec![
            entry("twitch", true, Some("twitch"), &["message", "donation"]),
            entry("da", true, Some("donationalerts"), &["donation"]),
            entry("off", false, Some("fixture"), &["donation"]),
            entry("alerter", true, None, &[]),
        ];
        let platforms = platforms_with_kind(&entries, "donation");
        assert_eq!(platforms, vec!["twitch", "donationalerts"]);
    }
}
