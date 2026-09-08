use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const MAX_LEN: u32 = 256;
const STORM_N: u32 = 20;
const STORM_WINDOW: Duration = Duration::from_secs(1);

pub struct RandomGate {
    storm: Mutex<HashMap<String, Storm>>,
}

struct Storm {
    window_start: Instant,
    count: u32,
}

impl RandomGate {
    pub fn new() -> Self {
        Self {
            storm: Mutex::new(HashMap::new()),
        }
    }

    pub fn fill(&self, plugin_id: &str, len: u32) -> Result<Vec<u8>, String> {
        if len == 0 || len > MAX_LEN {
            return Err(format!("random len out of range (1..={MAX_LEN})"));
        }
        self.touch_storm(plugin_id)?;
        let mut buf = vec![0u8; len as usize];
        getrandom::fill(&mut buf).map_err(|err| format!("random unavailable: {err}"))?;
        Ok(buf)
    }

    fn touch_storm(&self, plugin_id: &str) -> Result<(), String> {
        let mut storm = self.storm.lock().map_err(|err| err.to_string())?;
        let now = Instant::now();
        let slot = storm.entry(plugin_id.to_string()).or_insert(Storm {
            window_start: now,
            count: 0,
        });
        if now.duration_since(slot.window_start) >= STORM_WINDOW {
            slot.window_start = now;
            slot.count = 0;
        }
        slot.count = slot.count.saturating_add(1);
        if slot.count > STORM_N {
            return Err("random too frequent".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_returns_requested_len() {
        let gate = RandomGate::new();
        let bytes = gate.fill("com.test.random", 16).unwrap();
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn rejects_zero_and_over_cap() {
        let gate = RandomGate::new();
        let zero = gate.fill("com.test.random", 0).unwrap_err();
        assert!(zero.contains("range"), "{zero}");
        let over = gate.fill("com.test.random", MAX_LEN + 1).unwrap_err();
        assert!(over.contains("range"), "{over}");
    }

    #[test]
    fn storm_rejects_burst() {
        let gate = RandomGate::new();
        for _ in 0..STORM_N {
            gate.fill("com.test.random", 1).unwrap();
        }
        let err = gate.fill("com.test.random", 1).unwrap_err();
        assert!(err.contains("frequent"), "{err}");
    }
}
