use crate::wait::{self, Ready};

/// Wait timer `ms`. `true` if the host signalled stop.
pub fn wait_backoff(ms: u32) -> bool {
    wait::set_timer(ms);
    loop {
        match wait::wait() {
            Ready::Stop => return true,
            Ready::Timer | Ready::Resume => return false,
            Ready::Act(req) => {
                #[cfg(any(feature = "emitter", feature = "connector"))]
                crate::chat_complete::complete(&req.id, Err("no connection"));
                #[cfg(not(any(feature = "emitter", feature = "connector")))]
                let _ = req;
            }
            Ready::WsText(_)
            | Ready::WsClosed(_)
            | Ready::Bus(_)
            | Ready::Settings
            | Ready::Ui(_)
            | Ready::MediaEnded(_)
            | Ready::AlertPlay(_)
            | Ready::AlertStop(_) => {}
        }
    }
}
