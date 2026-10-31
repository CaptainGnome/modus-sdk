//! Shared 401/403 recovery for connectors: introspect? → refresh → invalidate.
//! Soft-link: guests that never call these still load.

use crate::bindings::modus::abi::auth_token;
use crate::HostError;

pub fn is_auth_http(status: u16) -> bool {
    status == 401 || status == 403
}

/// Drop RAM cache, force refresh, update `token`.
///
/// - `introspect` active → `Err("auth active")` (do not revoke; API/scope issue).
/// - api-mode `no refresh` → `invalidate` → `Err("refresh revoked")`.
/// - revoked refresh → `invalidate` (idempotent) → `Err("refresh revoked")`.
/// - network/broker errors on refresh → propagate without invalidate.
pub fn recover_auth(account_id: &str, token: &mut String) -> Result<(), String> {
    match auth_token::introspect(account_id) {
        Ok(status) if status.active => return Err("auth active".into()),
        Ok(_) | Err(_) => {}
    }
    match auth_token::refresh(account_id) {
        Ok(next) => {
            *token = next;
            Ok(())
        }
        Err(err) if err == "no refresh" => {
            let _ = auth_token::invalidate(account_id);
            Err("refresh revoked".into())
        }
        Err(err) => {
            if HostError::classify(&err).is_stop() {
                let _ = auth_token::invalidate(account_id);
            }
            Err(err)
        }
    }
}

/// Run `op(token)` up to twice. On 401/403: recover once, then retry.
/// Second auth failure → invalidate → `Err("refresh revoked")`.
///
/// `op` returns `(status, body)`. Non-auth statuses are returned as-is (caller checks 2xx).
pub fn fetch_authed<T, F>(account_id: &str, token: &mut String, op: F) -> Result<(u16, T), String>
where
    F: FnMut(&str) -> Result<(u16, T), String>,
{
    fetch_authed_on(account_id, token, is_auth_http, op)
}

/// Like [`fetch_authed`], but only treats HTTP 401 as auth failure (403 often = scope/quota).
pub fn fetch_unauthorized<T, F>(
    account_id: &str,
    token: &mut String,
    op: F,
) -> Result<(u16, T), String>
where
    F: FnMut(&str) -> Result<(u16, T), String>,
{
    fetch_authed_on(account_id, token, |status| status == 401, op)
}

pub fn fetch_authed_on<T, F, P>(
    account_id: &str,
    token: &mut String,
    is_auth: P,
    mut op: F,
) -> Result<(u16, T), String>
where
    F: FnMut(&str) -> Result<(u16, T), String>,
    P: Fn(u16) -> bool,
{
    for attempt in 0..2 {
        let (status, body) = op(token)?;
        if !is_auth(status) {
            return Ok((status, body));
        }
        if attempt > 0 {
            let _ = auth_token::invalidate(account_id);
            return Err("refresh revoked".into());
        }
        recover_auth(account_id, token)?;
    }
    Err("refresh revoked".into())
}

#[cfg(test)]
mod tests {
    use super::is_auth_http;

    #[test]
    fn auth_http_codes() {
        assert!(is_auth_http(401));
        assert!(is_auth_http(403));
        assert!(!is_auth_http(500));
        assert!(!is_auth_http(200));
    }
}
