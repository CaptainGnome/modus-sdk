use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const I18N_PREFIX: &str = "assets/i18n/";
pub const I18N_MAX_BYTES: usize = 32 * 1024;
pub const DEFAULT_PACK_LOCALE: &str = "en";
const MAX_I18N_KEY: usize = 128;
const MAX_I18N_VALUE: usize = 512;

pub type Catalogs = BTreeMap<String, BTreeMap<String, String>>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LocalizedString {
    Plain(String),
    Key { key: String, fallback: String },
}

impl LocalizedString {
    pub fn fallback(&self) -> &str {
        match self {
            Self::Plain(text) => text.as_str(),
            Self::Key { fallback, .. } => fallback.as_str(),
        }
    }

    pub fn validate(&self, label: &str) -> Result<(), String> {
        match self {
            Self::Plain(text) => require_plain(text, label),
            Self::Key { key, fallback } => {
                require_i18n_key(key)?;
                require_plain(fallback, label)?;
                Ok(())
            }
        }
    }

    pub fn collect_key(&self, out: &mut Vec<String>) {
        if let Self::Key { key, .. } = self {
            if !out.iter().any(|item| item == key) {
                out.push(key.clone());
            }
        }
    }
}

pub fn parse_catalog(bytes: &[u8]) -> Result<BTreeMap<String, String>, String> {
    if bytes.len() > I18N_MAX_BYTES {
        return Err("i18n too large".into());
    }
    let raw: Value =
        serde_json::from_slice(bytes).map_err(|err| format!("i18n: {err}"))?;
    let object = raw
        .as_object()
        .ok_or_else(|| "i18n: object required".to_string())?;
    let mut out = BTreeMap::new();
    for (key, value) in object {
        require_i18n_key(key)?;
        let text = value
            .as_str()
            .ok_or_else(|| format!("i18n: value {key} must be a string"))?;
        require_plain(text, "i18n value")?;
        if text.len() > MAX_I18N_VALUE {
            return Err(format!("i18n: value {key} is too long"));
        }
        out.insert(key.clone(), text.to_string());
    }
    Ok(out)
}

pub fn locale_code_from_entry(name: &str) -> Option<&str> {
    let rest = name.strip_prefix(I18N_PREFIX)?;
    let code = rest.strip_suffix(".json")?;
    if code.is_empty() || code.contains('/') || code.contains('\\') {
        return None;
    }
    Some(code)
}

pub fn is_locale_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    if bytes.len() == 2 {
        return bytes[0].is_ascii_lowercase() && bytes[1].is_ascii_lowercase();
    }
    if bytes.len() == 5 && bytes[2] == b'-' {
        return bytes[0].is_ascii_lowercase()
            && bytes[1].is_ascii_lowercase()
            && bytes[3].is_ascii_uppercase()
            && bytes[4].is_ascii_uppercase();
    }
    false
}

pub fn require_i18n_key(raw: &str) -> Result<(), String> {
    if raw.is_empty() || raw.len() > MAX_I18N_KEY {
        return Err("i18n: bad key".into());
    }
    let bytes = raw.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return Err("i18n: bad key".into());
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'.')
    {
        return Err("i18n: bad key".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct LabelI18nRef {
    key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct LabelI18nEnvelope {
    i18n: LabelI18nRef,
}

pub fn encode_label_i18n(key: &str, params: Option<Value>) -> Result<String, String> {
    require_i18n_key(key)?;
    if let Some(Value::Object(_)) = params.as_ref() {
        // ok
    } else if params.is_some() {
        return Err("params must be a JSON object".into());
    }
    let envelope = LabelI18nEnvelope {
        i18n: LabelI18nRef {
            key: key.to_string(),
            params,
        },
    };
    serde_json::to_string(&envelope).map_err(|err| err.to_string())
}

fn require_plain(raw: &str, label: &str) -> Result<(), String> {
    if raw.contains('<') || raw.contains('>') {
        return Err(format!("HTML in {label}"));
    }
    Ok(())
}

pub fn validate_keys_against_en(keys: &[String], catalogs: &Catalogs) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(());
    }
    let en = catalogs
        .get(DEFAULT_PACK_LOCALE)
        .ok_or_else(|| "need assets/i18n/en.json".to_string())?;
    for key in keys {
        if !en.contains_key(key) {
            return Err(format!("i18n: missing key {key} in en.json"));
        }
    }
    Ok(())
}
