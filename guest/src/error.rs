/// Host errors. Literal strings are part of ABI 2; change them only with a coordinated host+SDK release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostError {
    Stopped,
    Grant,
    Revoked,
    Network,
    Other(String),
}

pub const BACKOFF_START_MS: u32 = 1_000;
pub const BACKOFF_MAX_MS: u32 = 30_000;

pub fn next_backoff_ms(current: u32) -> u32 {
    current.saturating_mul(2).min(BACKOFF_MAX_MS)
}

impl HostError {
    pub fn classify(err: &str) -> Self {
        if err == "stopped" {
            return Self::Stopped;
        }
        if err.starts_with("no grant ") {
            return Self::Grant;
        }
        if err == "refresh revoked" || err == "foreign account" {
            return Self::Revoked;
        }
        if is_network(err) {
            return Self::Network;
        }
        Self::Other(err.to_string())
    }

    pub fn is_stop(&self) -> bool {
        matches!(self, Self::Stopped | Self::Revoked)
    }
}

impl From<&str> for HostError {
    fn from(err: &str) -> Self {
        Self::classify(err)
    }
}

fn is_network(err: &str) -> bool {
    err == "https/wss only"
        || err == "ws only"
        || err == "http quota"
        || err == "ws quota"
        || err == "bridge quota"
        || err == "body too large"
        || err == "response too large"
        || err == "literal IP forbidden"
        || err == "no tcp for ws"
        || err.contains("not in manifest")
        || err.contains("not in Core whitelist")
        || err.starts_with("forbidden address ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_host_strings() {
        assert_eq!(HostError::classify("stopped"), HostError::Stopped);
        assert_eq!(
            HostError::classify("no grant bus.emit"),
            HostError::Grant
        );
        assert_eq!(HostError::classify("no grant net.ws"), HostError::Grant);
        assert_eq!(HostError::classify("no grant net.bridge"), HostError::Grant);
        assert_eq!(HostError::classify("ws only"), HostError::Network);
        assert_eq!(HostError::classify("bridge quota"), HostError::Network);
        assert_eq!(HostError::classify("refresh revoked"), HostError::Revoked);
        assert_eq!(HostError::classify("foreign account"), HostError::Revoked);
        assert_eq!(HostError::classify("https/wss only"), HostError::Network);
        assert_eq!(HostError::classify("http quota"), HostError::Network);
        assert_eq!(
            HostError::classify("host cdn.example.com not in manifest"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("host api.example.com not in Core whitelist"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("literal IP forbidden"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("forbidden address 127.0.0.1"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("TooLarge"),
            HostError::Other("TooLarge".into())
        );
        assert_eq!(
            HostError::classify("no platform_id"),
            HostError::Other("no platform_id".into())
        );
        assert_eq!(
            HostError::classify("system is Core-only"),
            HostError::Other("system is Core-only".into())
        );
    }

    #[test]
    fn stop_is_stopped_or_revoked() {
        assert!(HostError::Stopped.is_stop());
        assert!(HostError::Revoked.is_stop());
        assert!(!HostError::Grant.is_stop());
        assert!(!HostError::Network.is_stop());
        assert!(!HostError::Other("ws closed".into()).is_stop());
    }

    #[test]
    fn backoff_doubles_until_cap() {
        assert_eq!(next_backoff_ms(1_000), 2_000);
        assert_eq!(next_backoff_ms(16_000), 30_000);
        assert_eq!(next_backoff_ms(30_000), 30_000);
        assert_eq!(next_backoff_ms(u32::MAX), BACKOFF_MAX_MS);
    }
}
