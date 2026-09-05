use super::event::Event;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const PAGE_MAX: u32 = 50;
pub const RING_MAX: usize = 500;
const STORM_N: u32 = 2;
const STORM_WINDOW: Duration = Duration::from_secs(1);

pub struct HistoryGate {
    storm: Mutex<HashMap<String, Storm>>,
}

struct Storm {
    window_start: Instant,
    count: u32,
}

impl HistoryGate {
    pub fn new() -> Self {
        Self {
            storm: Mutex::new(HashMap::new()),
        }
    }

    pub fn touch(&self, plugin_id: &str) -> Result<(), String> {
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
            return Err("слишком часто history".into());
        }
        Ok(())
    }
}

pub fn encode_cursor(ts: i64, id: &str) -> String {
    format!("{ts}:{id}")
}

pub fn decode_cursor(raw: &str) -> Result<(i64, String), String> {
    let (ts, id) = raw.split_once(':').ok_or_else(|| "неверный курсор".to_string())?;
    if ts.is_empty() || id.is_empty() {
        return Err("неверный курсор".into());
    }
    let ts: i64 = ts.parse().map_err(|_| "неверный курсор".to_string())?;
    Ok((ts, id.to_string()))
}

pub fn page_ring(
    events: &VecDeque<Event>,
    cursor: Option<&str>,
    limit: u32,
) -> Result<(Vec<Event>, Option<String>), String> {
    let before = match cursor {
        Some(raw) => Some(decode_cursor(raw)?),
        None => None,
    };
    let limit = limit.clamp(1, PAGE_MAX) as usize;
    let mut filtered: Vec<&Event> = events
        .iter()
        .filter(|event| match &before {
            None => true,
            Some((ts, id)) => {
                let eid = event.id.to_string();
                event.ts < *ts || (event.ts == *ts && eid.as_str() < id.as_str())
            }
        })
        .collect();
    filtered.sort_by(|a, b| {
        a.ts.cmp(&b.ts)
            .then_with(|| a.id.to_string().cmp(&b.id.to_string()))
    });
    let skip = filtered.len().saturating_sub(limit);
    let page: Vec<Event> = filtered[skip..].iter().map(|event| (*event).clone()).collect();
    let next = if page.len() == limit {
        page.first()
            .map(|event| encode_cursor(event.ts, &event.id.to_string()))
    } else {
        None
    };
    Ok((page, next))
}
