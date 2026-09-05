use super::event::{
    kind_as_str, payload_text, plugin_emit_request, EmitRequest, Event, Payload,
};
use super::history::{page_ring, RING_MAX};
use super::mailbox::{Mail, Mailbox};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct Bus {
    mailbox: Arc<Mailbox>,
    subscribed: AtomicBool,
    log: Mutex<VecDeque<Event>>,
}

impl Bus {
    pub fn new(mailbox: Arc<Mailbox>) -> Self {
        Self {
            mailbox,
            subscribed: AtomicBool::new(false),
            log: Mutex::new(VecDeque::new()),
        }
    }

    pub fn subscribe(&self) {
        self.subscribed.store(true, Ordering::SeqCst);
    }

    pub fn emit_from_plugin(
        &self,
        plugin_id: String,
        platform: Option<String>,
        channel: String,
        payload: Payload,
        opaque: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let req = plugin_emit_request(plugin_id, platform, channel, payload, opaque)?;
        self.dispatch(req)
    }

    pub fn emit_host(&self, req: EmitRequest) -> Result<(), String> {
        self.dispatch(req)
    }

    pub fn history_page(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<Event>, Option<String>), String> {
        let log = self.log.lock().map_err(|err| err.to_string())?;
        page_ring(&log, cursor, limit)
    }

    fn dispatch(&self, req: EmitRequest) -> Result<(), String> {
        let event = Event::stamp(req).map_err(|err| err.to_string())?;
        eprintln!(
            "emit {} {}/{} {}",
            kind_as_str(&event.kind),
            event.source.plugin_id,
            event.source.channel,
            payload_text(&event.payload)
        );
        let _ = io::stderr().flush();
        {
            let mut log = self.log.lock().map_err(|err| err.to_string())?;
            if log.len() >= RING_MAX {
                log.pop_front();
            }
            log.push_back(event.clone());
        }
        if !self.subscribed.load(Ordering::SeqCst) {
            return Ok(());
        }
        match self.mailbox.sender().try_send(Mail::Bus(event)) {
            Ok(()) => Ok(()),
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                eprintln!("шина: inbox полный, drop");
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => Ok(()),
        }
    }
}
