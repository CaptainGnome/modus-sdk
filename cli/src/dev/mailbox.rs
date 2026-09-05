use super::act::ActRequest;
use super::event::Event;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const INBOX_CAP: usize = 64;

#[derive(Debug)]
pub enum Mail {
    WsText { handle: u32, text: String },
    WsClosed(u32),
    Bus(Event),
    Timer,
    Stop,
    Ui(Vec<u8>),
    MediaEnded(String),
    Settings,
    Act(ActRequest),
}

pub struct Mailbox {
    tx: SyncSender<Mail>,
    rx: Mutex<Receiver<Mail>>,
    timer: Mutex<Option<Instant>>,
}

impl Mailbox {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(INBOX_CAP);
        Self {
            tx,
            rx: Mutex::new(rx),
            timer: Mutex::new(None),
        }
    }

    pub fn sender(&self) -> SyncSender<Mail> {
        self.tx.clone()
    }

    pub fn set_timer(&self, ms: u32) {
        let mut timer = self.timer.lock().expect("timer");
        *timer = if ms == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_millis(ms as u64))
        };
    }

    pub fn wake_stop(&self) {
        let _ = self.tx.try_send(Mail::Stop);
    }

    pub fn wake_media_ended(&self, id: String) {
        let _ = self.tx.try_send(Mail::MediaEnded(id));
    }

    pub fn wait(&self, stop: &AtomicBool) -> Mail {
        let rx = self.rx.lock().expect("mailbox rx");
        loop {
            if stop.load(Ordering::SeqCst) {
                return Mail::Stop;
            }
            match rx.try_recv() {
                Ok(Mail::Stop) => return Mail::Stop,
                Ok(mail) => {
                    if stop.load(Ordering::SeqCst) {
                        return Mail::Stop;
                    }
                    return mail;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return Mail::Stop,
            }
            let timeout = {
                let mut timer = self.timer.lock().expect("timer");
                if let Some(deadline) = *timer {
                    let now = Instant::now();
                    if deadline <= now {
                        *timer = None;
                        return Mail::Timer;
                    }
                    deadline.saturating_duration_since(now)
                } else {
                    Duration::from_millis(50)
                }
            };
            match rx.recv_timeout(timeout.min(Duration::from_millis(50))) {
                Ok(Mail::Stop) => return Mail::Stop,
                Ok(mail) => {
                    if stop.load(Ordering::SeqCst) {
                        return Mail::Stop;
                    }
                    return mail;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let mut timer = self.timer.lock().expect("timer");
                    if let Some(deadline) = *timer {
                        if deadline <= Instant::now() {
                            *timer = None;
                            if stop.load(Ordering::SeqCst) {
                                return Mail::Stop;
                            }
                            return Mail::Timer;
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return Mail::Stop,
            }
        }
    }
}
