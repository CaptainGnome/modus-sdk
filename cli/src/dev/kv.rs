use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const KV_QUOTA: usize = 256 * 1024;
pub const KV_MAX_KEYS: usize = 256;
pub const KV_MAX_VALUE: usize = 16 * 1024;
const STORM_N: u32 = 60;
const STORM_WINDOW: Duration = Duration::from_secs(1);

pub struct DevKv {
    map: Mutex<HashMap<String, String>>,
    storm: Mutex<Storm>,
}

struct Storm {
    window_start: Instant,
    count: u32,
}

impl DevKv {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            storm: Mutex::new(Storm {
                window_start: Instant::now(),
                count: 0,
            }),
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        if !is_kv_key(key) {
            return Err("плохой ключ kv".into());
        }
        let map = self.map.lock().map_err(|err| err.to_string())?;
        Ok(map.get(key).cloned())
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.touch_storm()?;
        if !is_kv_key(key) {
            return Err("плохой ключ kv".into());
        }
        if value.len() > KV_MAX_VALUE {
            return Err("значение kv слишком большое".into());
        }
        let mut map = self.map.lock().map_err(|err| err.to_string())?;
        let old_size = map
            .get(key)
            .map(|v| key.len() + v.len())
            .unwrap_or(0);
        let new_size = key.len() + value.len();
        let used: usize = map.iter().map(|(k, v)| k.len() + v.len()).sum();
        let next = used.saturating_sub(old_size).saturating_add(new_size);
        if next > KV_QUOTA {
            return Err("квота kv".into());
        }
        if !map.contains_key(key) && map.len() >= KV_MAX_KEYS {
            return Err("слишком много ключей kv".into());
        }
        map.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<(), String> {
        self.touch_storm()?;
        if !is_kv_key(key) {
            return Err("плохой ключ kv".into());
        }
        let mut map = self.map.lock().map_err(|err| err.to_string())?;
        map.remove(key);
        Ok(())
    }

    pub fn list_keys(&self, prefix: &str) -> Result<Vec<String>, String> {
        if prefix.len() > 128 {
            return Err("плохой префикс kv".into());
        }
        let map = self.map.lock().map_err(|err| err.to_string())?;
        let mut out: Vec<String> = map
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        out.sort();
        out.truncate(KV_MAX_KEYS);
        Ok(out)
    }

    fn touch_storm(&self) -> Result<(), String> {
        let mut storm = self.storm.lock().map_err(|err| err.to_string())?;
        let now = Instant::now();
        if now.duration_since(storm.window_start) >= STORM_WINDOW {
            storm.window_start = now;
            storm.count = 0;
        }
        storm.count = storm.count.saturating_add(1);
        if storm.count > STORM_N {
            return Err("слишком часто kv".into());
        }
        Ok(())
    }
}

pub fn is_kv_key(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete() {
        let kv = DevKv::new();
        assert_eq!(kv.get("boots").unwrap(), None);
        kv.set("boots", "1").unwrap();
        assert_eq!(kv.get("boots").unwrap().as_deref(), Some("1"));
        kv.delete("boots").unwrap();
        assert_eq!(kv.get("boots").unwrap(), None);
    }

    #[test]
    fn rejects_bad_key_and_quota() {
        let kv = DevKv::new();
        assert!(kv.set("has space", "x").unwrap_err().contains("плохой"));
        let big = "x".repeat(KV_MAX_VALUE + 1);
        assert!(kv.set("k", &big).unwrap_err().contains("большое"));
    }

    #[test]
    fn list_prefix() {
        let kv = DevKv::new();
        kv.set("a.1", "1").unwrap();
        kv.set("a.2", "2").unwrap();
        kv.set("b.1", "3").unwrap();
        assert_eq!(kv.list_keys("a.").unwrap(), vec!["a.1", "a.2"]);
    }

    #[test]
    fn storm_rejects_burst() {
        let kv = DevKv::new();
        for i in 0..STORM_N {
            kv.set(&format!("k{i}"), "v").unwrap();
        }
        let err = kv.set("overflow", "v").unwrap_err();
        assert!(err.contains("часто"), "{err}");
    }
}
