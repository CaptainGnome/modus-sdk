/// Ошибки хоста. Набор строк — часть ABI 2; смена без мажора SDK ломает enum.
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
        if err == "остановлен" {
            return Self::Stopped;
        }
        if err.starts_with("нет гранта ") {
            return Self::Grant;
        }
        if err == "refresh отозван" || err == "чужой аккаунт" {
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
    err == "только https/wss"
        || err == "квота http"
        || err == "квота ws"
        || err == "тело слишком большое"
        || err == "ответ слишком большой"
        || err == "литеральный IP запрещён"
        || err == "нет tcp для ws"
        || err.contains("вне манифеста")
        || err.contains("не в whitelist Core")
        || err.starts_with("запрещённый адрес ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_host_strings() {
        assert_eq!(HostError::classify("остановлен"), HostError::Stopped);
        assert_eq!(
            HostError::classify("нет гранта bus.emit"),
            HostError::Grant
        );
        assert_eq!(
            HostError::classify("нет гранта net.ws"),
            HostError::Grant
        );
        assert_eq!(HostError::classify("refresh отозван"), HostError::Revoked);
        assert_eq!(HostError::classify("чужой аккаунт"), HostError::Revoked);
        assert_eq!(HostError::classify("только https/wss"), HostError::Network);
        assert_eq!(HostError::classify("квота http"), HostError::Network);
        assert_eq!(
            HostError::classify("хост cdn.example.com вне манифеста"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("хост api.example.com не в whitelist Core"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("литеральный IP запрещён"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("запрещённый адрес 127.0.0.1"),
            HostError::Network
        );
        assert_eq!(
            HostError::classify("TooLarge"),
            HostError::Other("TooLarge".into())
        );
        assert_eq!(
            HostError::classify("нет platform_id"),
            HostError::Other("нет platform_id".into())
        );
        assert_eq!(
            HostError::classify("system только Core"),
            HostError::Other("system только Core".into())
        );
    }

    #[test]
    fn stop_is_stopped_or_revoked() {
        assert!(HostError::Stopped.is_stop());
        assert!(HostError::Revoked.is_stop());
        assert!(!HostError::Grant.is_stop());
        assert!(!HostError::Network.is_stop());
        assert!(!HostError::Other("ws закрыт".into()).is_stop());
    }

    #[test]
    fn backoff_doubles_until_cap() {
        assert_eq!(next_backoff_ms(1_000), 2_000);
        assert_eq!(next_backoff_ms(16_000), 30_000);
        assert_eq!(next_backoff_ms(30_000), 30_000);
        assert_eq!(next_backoff_ms(u32::MAX), BACKOFF_MAX_MS);
    }
}
