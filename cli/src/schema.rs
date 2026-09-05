use crate::i18n::LocalizedString;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_MAX_BYTES: usize = 32 * 1024;
const MAX_SECTIONS: usize = 8;
const MAX_FIELDS: usize = 32;
const MAX_STRING: usize = 1024;
const MAX_LIST_ITEMS: usize = 32;
const MAX_LIST_ITEM: usize = 256;
const MAX_ENUM_OPTIONS: usize = 16;
const MAX_HELP: usize = 256;
const MAX_LABEL: usize = 128;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FieldType {
    String,
    Number,
    Bool,
    Enum,
    List,
    Secret,
    Label,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct EnumOption {
    value: String,
    label: LocalizedString,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SettingsField {
    key: String,
    #[serde(rename = "type")]
    field_type: FieldType,
    label: LocalizedString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    help: Option<LocalizedString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_len: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    options: Vec<EnumOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_items: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<LocalizedString>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SettingsSection {
    id: String,
    title: LocalizedString,
    fields: Vec<SettingsField>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SettingsSchema {
    version: u32,
    sections: Vec<SettingsSection>,
}

impl SettingsSchema {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > SCHEMA_MAX_BYTES {
            return Err("settings.json слишком большой".into());
        }
        let schema: SettingsSchema =
            serde_json::from_slice(bytes).map_err(|err| format!("settings.json: {err}"))?;
        schema.validate()?;
        Ok(schema)
    }

    pub fn has_field(&self, key: &str) -> bool {
        self.field(key).is_some()
    }

    pub fn is_label(&self, key: &str) -> bool {
        self.field(key)
            .is_some_and(|field| field.field_type == FieldType::Label)
    }

    pub fn default_stored(&self, key: &str) -> Option<String> {
        let field = self.field(key)?;
        match field.field_type {
            FieldType::Label => field
                .text
                .as_ref()
                .map(|text| text.fallback().to_string()),
            FieldType::Secret => None,
            FieldType::String => field
                .default
                .as_ref()
                .and_then(|v| v.as_str().map(str::to_string))
                .or_else(|| Some(String::new())),
            FieldType::Number => field
                .default
                .as_ref()
                .and_then(|v| as_finite_number(v).ok())
                .and_then(number_to_canonical),
            FieldType::Bool => field.default.as_ref().and_then(|v| {
                v.as_bool()
                    .map(|flag| if flag { "true".into() } else { "false".into() })
            }),
            FieldType::Enum => field
                .default
                .as_ref()
                .and_then(|v| v.as_str().map(str::to_string)),
            FieldType::List => field.default.as_ref().and_then(|v| {
                let items = v.as_array()?;
                let strings: Vec<&str> = items.iter().filter_map(|item| item.as_str()).collect();
                if strings.len() != items.len() {
                    return None;
                }
                serde_json::to_string(&strings).ok()
            }),
        }
    }

    pub fn validate_value(&self, key: &str, value: &Value) -> Result<String, String> {
        let field = self.field(key).ok_or_else(|| format!("нет поля {key}"))?;
        match field.field_type {
            FieldType::String => {
                let text = value.as_str().ok_or("нужна строка")?;
                let cap = field.max_len.unwrap_or(MAX_STRING as u32) as usize;
                if text.len() > cap.min(MAX_STRING) {
                    return Err("строка слишком длинная".into());
                }
                Ok(text.to_string())
            }
            FieldType::Number => {
                let n = as_finite_number(value)?;
                if let Some(min) = field.min {
                    if n < min {
                        return Err("число вне диапазона".into());
                    }
                }
                if let Some(max) = field.max {
                    if n > max {
                        return Err("число вне диапазона".into());
                    }
                }
                number_to_canonical(n).ok_or_else(|| "число".into())
            }
            FieldType::Bool => {
                let flag = value.as_bool().ok_or("нужен bool")?;
                Ok(if flag { "true".into() } else { "false".into() })
            }
            FieldType::Enum => {
                let text = value.as_str().ok_or("нужна строка")?;
                if !field.options.iter().any(|option| option.value == text) {
                    return Err("значение не в enum".into());
                }
                Ok(text.to_string())
            }
            FieldType::List => {
                let items = value.as_array().ok_or("нужен список")?;
                let cap = field.max_items.unwrap_or(MAX_LIST_ITEMS as u32) as usize;
                if items.len() > cap.min(MAX_LIST_ITEMS) {
                    return Err("список слишком длинный".into());
                }
                let mut out = Vec::new();
                for item in items {
                    let text = item.as_str().ok_or("элемент списка — строка")?;
                    if text.len() > MAX_LIST_ITEM {
                        return Err("элемент списка слишком длинный".into());
                    }
                    out.push(text.to_string());
                }
                serde_json::to_string(&out).map_err(|err| err.to_string())
            }
            FieldType::Secret => {
                let text = value.as_str().ok_or("нужна строка")?;
                if text.len() > 512 {
                    return Err("секрет слишком длинный".into());
                }
                Ok(text.to_string())
            }
            FieldType::Label => Err("label только через set-label".into()),
        }
    }

    fn field(&self, key: &str) -> Option<&SettingsField> {
        self.sections
            .iter()
            .flat_map(|section| section.fields.iter())
            .find(|field| field.key == key)
    }

    pub fn all_keys(&self) -> Vec<String> {
        self.sections
            .iter()
            .flat_map(|section| section.fields.iter())
            .map(|field| field.key.clone())
            .collect()
    }

    pub fn i18n_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in &self.sections {
            section.title.collect_key(&mut out);
            for field in &section.fields {
                field.label.collect_key(&mut out);
                if let Some(help) = &field.help {
                    help.collect_key(&mut out);
                }
                if let Some(text) = &field.text {
                    text.collect_key(&mut out);
                }
                for option in &field.options {
                    option.label.collect_key(&mut out);
                }
            }
        }
        out
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("settings.json: нужен version 1".into());
        }
        if self.sections.len() > MAX_SECTIONS {
            return Err("settings.json: слишком много секций".into());
        }
        let mut keys = Vec::new();
        let mut section_ids = Vec::new();
        for section in &self.sections {
            require_key(&section.id, "id секции")?;
            section
                .title
                .validate("title секции")
                .map_err(|err| format!("settings.json: {err}"))?;
            if section_ids.contains(&section.id) {
                return Err(format!("settings.json: дубль секции {}", section.id));
            }
            section_ids.push(section.id.clone());
            for field in &section.fields {
                field.validate()?;
                if keys.iter().any(|k| k == &field.key) {
                    return Err(format!("settings.json: дубль ключа {}", field.key));
                }
                keys.push(field.key.clone());
            }
        }
        if keys.len() > MAX_FIELDS {
            return Err("settings.json: слишком много полей".into());
        }
        Ok(())
    }
}

impl SettingsField {
    fn validate(&self) -> Result<(), String> {
        require_key(&self.key, "ключ")?;
        self.label
            .validate("label")
            .map_err(|err| format!("settings.json: {err}"))?;
        if let Some(help) = &self.help {
            help.validate("help")
                .map_err(|err| format!("settings.json: {err}"))?;
            if help.fallback().len() > MAX_HELP {
                return Err("settings.json: help слишком длинный".into());
            }
        }
        match self.field_type {
            FieldType::String => {
                if let Some(default) = &self.default {
                    let text = default
                        .as_str()
                        .ok_or("settings.json: default string — строка")?;
                    let cap = self.max_len.unwrap_or(MAX_STRING as u32) as usize;
                    if cap > MAX_STRING || text.len() > cap.min(MAX_STRING) {
                        return Err("settings.json: default string слишком длинный".into());
                    }
                }
                if self.max_len.unwrap_or(MAX_STRING as u32) as usize > MAX_STRING {
                    return Err("settings.json: max_len слишком большой".into());
                }
            }
            FieldType::Number => {
                if let Some(default) = &self.default {
                    let n = as_finite_number(default)?;
                    check_number_range(n, self.min, self.max)?;
                }
                if let (Some(min), Some(max)) = (self.min, self.max) {
                    if min > max {
                        return Err("settings.json: min > max".into());
                    }
                }
            }
            FieldType::Bool => {
                if let Some(default) = &self.default {
                    if !default.is_boolean() {
                        return Err("settings.json: default bool".into());
                    }
                }
            }
            FieldType::Enum => {
                if self.options.is_empty() || self.options.len() > MAX_ENUM_OPTIONS {
                    return Err("settings.json: enum options".into());
                }
                let mut values = Vec::new();
                for option in &self.options {
                    option
                        .label
                        .validate("enum label")
                        .map_err(|err| format!("settings.json: {err}"))?;
                    if option.value.is_empty() || option.value.len() > 64 {
                        return Err("settings.json: enum value".into());
                    }
                    require_plain(&option.value, "enum value")?;
                    if values.contains(&option.value) {
                        return Err("settings.json: дубль enum value".into());
                    }
                    values.push(option.value.clone());
                }
                if let Some(default) = &self.default {
                    let value = default
                        .as_str()
                        .ok_or("settings.json: default enum — строка")?;
                    if !values.iter().any(|item| item == value) {
                        return Err("settings.json: default не в options".into());
                    }
                }
            }
            FieldType::List => {
                let cap = self.max_items.unwrap_or(MAX_LIST_ITEMS as u32) as usize;
                if cap > MAX_LIST_ITEMS {
                    return Err("settings.json: max_items слишком большой".into());
                }
                if let Some(default) = &self.default {
                    let items = default
                        .as_array()
                        .ok_or("settings.json: default list — массив")?;
                    if items.len() > cap {
                        return Err("settings.json: default list слишком длинный".into());
                    }
                    for item in items {
                        let text = item
                            .as_str()
                            .ok_or("settings.json: элемент list — строка")?;
                        if text.len() > MAX_LIST_ITEM {
                            return Err("settings.json: элемент list слишком длинный".into());
                        }
                    }
                }
            }
            FieldType::Secret => {
                if self.default.is_some() {
                    return Err("settings.json: secret без default".into());
                }
            }
            FieldType::Label => {
                if self.default.is_some() {
                    return Err("settings.json: label без default".into());
                }
                if let Some(text) = &self.text {
                    text.validate("label text")
                        .map_err(|err| format!("settings.json: {err}"))?;
                    if text.fallback().len() > MAX_LABEL {
                        return Err("settings.json: label слишком длинный".into());
                    }
                }
            }
        }
        Ok(())
    }
}

fn number_to_canonical(n: f64) -> Option<String> {
    serde_json::Number::from_f64(n).map(|num| num.to_string())
}

fn as_finite_number(value: &Value) -> Result<f64, String> {
    let n = match value {
        Value::Number(num) => num.as_f64().ok_or("число")?,
        _ => return Err("нужно число".into()),
    };
    if !n.is_finite() {
        return Err("число не конечное".into());
    }
    Ok(n)
}

fn check_number_range(n: f64, min: Option<f64>, max: Option<f64>) -> Result<(), String> {
    if min.is_some_and(|min| n < min) || max.is_some_and(|max| n > max) {
        return Err("число вне диапазона".into());
    }
    Ok(())
}

fn require_key(raw: &str, label: &str) -> Result<(), String> {
    if !is_schema_key(raw) {
        return Err(format!("settings.json: плохой {label}"));
    }
    Ok(())
}

fn is_schema_key(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

fn require_plain(raw: &str, label: &str) -> Result<(), String> {
    if raw.contains('<') || raw.contains('>') {
        return Err(format!("settings.json: HTML в {label}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_schema() -> &'static [u8] {
        br#"{
            "version": 1,
            "sections": [{
                "id": "main",
                "title": "Main",
                "fields": [
                    {"key": "note", "type": "string", "label": "Note", "default": ""},
                    {"key": "echo", "type": "bool", "label": "Echo", "default": false},
                    {"key": "token", "type": "secret", "label": "Token"},
                    {"key": "status", "type": "label", "label": "Status", "text": "none"}
                ]
            }]
        }"#
    }

    #[test]
    fn parses_store_schema() {
        let schema = SettingsSchema::parse(ok_schema()).unwrap();
        assert_eq!(schema.sections.len(), 1);
        assert_eq!(schema.sections[0].fields.len(), 4);
        assert!(schema
            .sections[0]
            .fields
            .iter()
            .any(|field| field.key == "token"));
    }

    #[test]
    fn rejects_secret_default() {
        let raw = br#"{
            "version": 1,
            "sections": [{
                "id": "main",
                "title": "x",
                "fields": [{"key": "token", "type": "secret", "label": "Token", "default": "leak"}]
            }]
        }"#;
        let err = SettingsSchema::parse(raw).unwrap_err();
        assert!(err.contains("secret"), "{err}");
    }

    #[test]
    fn rejects_html_label() {
        let raw = br#"{
            "version": 1,
            "sections": [{
                "id": "main",
                "title": "x",
                "fields": [{"key": "note", "type": "string", "label": "<b>x</b>"}]
            }]
        }"#;
        assert!(SettingsSchema::parse(raw).unwrap_err().contains("HTML"));
    }

    #[test]
    fn rejects_duplicate_key() {
        let raw = br#"{
            "version": 1,
            "sections": [{
                "id": "main",
                "title": "x",
                "fields": [
                    {"key": "note", "type": "string", "label": "a"},
                    {"key": "note", "type": "bool", "label": "b"}
                ]
            }]
        }"#;
        assert!(SettingsSchema::parse(raw).unwrap_err().contains("дубль"));
    }

    #[test]
    fn collects_i18n_keys() {
        let schema = SettingsSchema::parse(
            br#"{
                "version": 1,
                "sections": [{
                    "id": "main",
                    "title": {"key": "settings.section", "fallback": "Main"},
                    "fields": [{
                        "key": "note",
                        "type": "string",
                        "label": {"key": "settings.note", "fallback": "Note"}
                    }]
                }]
            }"#,
        )
        .unwrap();
        let keys = schema.i18n_keys();
        assert!(keys.contains(&"settings.section".to_string()));
        assert!(keys.contains(&"settings.note".to_string()));
    }
}
