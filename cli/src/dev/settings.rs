use crate::i18n::encode_label_i18n;
use crate::schema::SettingsSchema;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

const MAX_LABEL: usize = 128;

pub struct DevSettings {
    schema: Option<SettingsSchema>,
    values: Mutex<HashMap<String, String>>,
}

impl DevSettings {
    pub fn load(plugin_dir: &Path, overlay: Option<&Path>) -> Result<(Self, bool), String> {
        let schema_path = plugin_dir.join("assets/settings.json");
        let schema = if schema_path.is_file() {
            let bytes = fs::read(&schema_path).map_err(|err| format!("settings.json: {err}"))?;
            Some(SettingsSchema::parse(&bytes)?)
        } else {
            None
        };

        let mut values = HashMap::new();
        if let Some(schema) = &schema {
            for key in schema_keys(schema) {
                if let Some(stored) = schema.default_stored(&key) {
                    values.insert(key, stored);
                }
            }
        }

        let wake = if let Some(path) = overlay {
            let raw = fs::read_to_string(path).map_err(|err| format!("settings: {err}"))?;
            let obj: Value =
                serde_json::from_str(raw.trim()).map_err(|err| format!("settings JSON: {err}"))?;
            let map = obj
                .as_object()
                .ok_or_else(|| "settings: нужен JSON-объект".to_string())?;
            let Some(schema) = &schema else {
                return Err("settings: нет assets/settings.json".into());
            };
            for (key, value) in map {
                if !schema.has_field(key) {
                    return Err(format!("settings: нет поля {key}"));
                }
                let stored = schema.validate_value(key, value)?;
                values.insert(key.clone(), stored);
            }
            true
        } else {
            false
        };

        Ok((
            Self {
                schema,
                values: Mutex::new(values),
            },
            wake,
        ))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let schema = self.schema.as_ref()?;
        if !schema.has_field(key) {
            return None;
        }
        let values = self.values.lock().ok()?;
        values
            .get(key)
            .cloned()
            .or_else(|| schema.default_stored(key))
    }

    pub fn set_label(&self, key: &str, text: &str) -> Result<(), String> {
        let schema = self.schema.as_ref().ok_or("нет схемы settings")?;
        if !schema.is_label(key) {
            if schema.has_field(key) {
                return Err("не label".into());
            }
            return Err("нет поля".into());
        }
        if text.contains('<') || text.contains('>') {
            return Err("HTML в label".into());
        }
        if text.len() > MAX_LABEL {
            return Err("label слишком длинный".into());
        }
        let mut values = self.values.lock().map_err(|err| err.to_string())?;
        values.insert(key.to_string(), text.to_string());
        Ok(())
    }

    pub fn set_label_i18n(
        &self,
        key: &str,
        label_key: &str,
        params: Option<String>,
    ) -> Result<(), String> {
        let schema = self.schema.as_ref().ok_or("нет схемы settings")?;
        if !schema.is_label(key) {
            if schema.has_field(key) {
                return Err("не label".into());
            }
            return Err("нет поля".into());
        }
        let params_value = match params.as_deref() {
            None => None,
            Some(raw) => {
                let value: Value =
                    serde_json::from_str(raw).map_err(|_| "params — JSON".to_string())?;
                if !value.is_object() {
                    return Err("params — JSON-объект".into());
                }
                Some(value)
            }
        };
        let encoded = encode_label_i18n(label_key, params_value)?;
        if encoded.len() > MAX_LABEL * 8 {
            return Err("label i18n слишком длинный".into());
        }
        let mut values = self.values.lock().map_err(|err| err.to_string())?;
        values.insert(key.to_string(), encoded);
        Ok(())
    }
}

fn schema_keys(schema: &SettingsSchema) -> Vec<String> {
    schema.all_keys()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn defaults_and_overlay() {
        let dir = tempdir().unwrap();
        let assets = dir.path().join("assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            assets.join("settings.json"),
            br#"{
              "version": 1,
              "sections": [{
                "id": "main",
                "title": "Main",
                "fields": [
                  {"key": "note", "type": "string", "label": "Note", "default": ""},
                  {"key": "echo", "type": "bool", "label": "Echo", "default": false},
                  {"key": "token", "type": "secret", "label": "Token"},
                  {"key": "status", "type": "label", "label": "Status", "text": "idle"}
                ]
              }]
            }"#,
        )
        .unwrap();
        let (settings, wake) = DevSettings::load(dir.path(), None).unwrap();
        assert!(!wake);
        assert_eq!(settings.get("note").as_deref(), Some(""));
        assert_eq!(settings.get("echo").as_deref(), Some("false"));
        assert_eq!(settings.get("token"), None);
        assert_eq!(settings.get("status").as_deref(), Some("idle"));

        let overlay = dir.path().join("overlay.json");
        let mut file = fs::File::create(&overlay).unwrap();
        write!(file, r#"{{"note":"hi","echo":true,"token":"secret"}}"#).unwrap();
        let (settings, wake) = DevSettings::load(dir.path(), Some(&overlay)).unwrap();
        assert!(wake);
        assert_eq!(settings.get("note").as_deref(), Some("hi"));
        assert_eq!(settings.get("echo").as_deref(), Some("true"));
        assert_eq!(settings.get("token").as_deref(), Some("secret"));
    }
}
