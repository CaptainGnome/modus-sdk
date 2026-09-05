use serde_json::Value;
use std::fs;
use std::path::Path;

const MAX_TEXT: usize = 500;
const MAX_DURATION_SEC: u32 = 1_209_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActKind {
    Send,
    Delete,
    Timeout,
    Ban,
    Unban,
}

#[derive(Clone, Debug)]
pub struct ActRequest {
    pub id: String,
    pub account_id: String,
    pub platform: String,
    pub channel: String,
    pub kind: ActKind,
    pub text: Option<String>,
    pub message_id: Option<String>,
    pub target_user_id: Option<String>,
    pub duration_sec: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ActJob {
    pub platform: String,
    pub channel: String,
    pub kind: ActKind,
    pub text: Option<String>,
    pub message_id: Option<String>,
    pub target_user_id: Option<String>,
    pub duration_sec: Option<u32>,
}

impl ActJob {
    pub fn validate(&mut self) -> Result<(), String> {
        match self.kind {
            ActKind::Send => {
                let text = self.text.as_deref().unwrap_or("").trim();
                if text.is_empty() {
                    return Err("empty message".into());
                }
                if text.len() > MAX_TEXT {
                    return Err("too long".into());
                }
                self.text = Some(text.to_string());
            }
            ActKind::Delete => {
                let id = self.message_id.as_deref().unwrap_or("").trim();
                if id.is_empty() {
                    return Err("missing message-id".into());
                }
                self.message_id = Some(id.to_string());
            }
            ActKind::Timeout | ActKind::Ban | ActKind::Unban => {
                let uid = self.target_user_id.as_deref().unwrap_or("").trim();
                if uid.is_empty() {
                    return Err("missing target-user-id".into());
                }
                self.target_user_id = Some(uid.to_string());
                if self.kind == ActKind::Timeout {
                    match self.duration_sec {
                        None | Some(0) => return Err("duration 0".into()),
                        Some(secs) => self.duration_sec = Some(secs.min(MAX_DURATION_SEC)),
                    }
                }
            }
        }
        Ok(())
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            ActKind::Send => "send",
            ActKind::Delete => "delete",
            ActKind::Timeout => "timeout",
            ActKind::Ban => "ban",
            ActKind::Unban => "unban",
        }
    }
}

pub fn load_acts(path: &Path, default_account: &str) -> Result<Vec<ActRequest>, String> {
    let raw = fs::read_to_string(path).map_err(|err| format!("act: {err}"))?;
    parse_acts(raw.trim(), default_account)
}

pub fn parse_acts(text: &str, default_account: &str) -> Result<Vec<ActRequest>, String> {
    if text.is_empty() {
        return Err("act: empty JSON".into());
    }
    let value: Value = serde_json::from_str(text).map_err(|err| format!("act JSON: {err}"))?;
    match value {
        Value::Array(items) => items
            .into_iter()
            .map(|item| parse_one(item, default_account))
            .collect(),
        other => Ok(vec![parse_one(other, default_account)?]),
    }
}

fn parse_one(mut value: Value, default_account: &str) -> Result<ActRequest, String> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "act: object required".to_string())?;
    let id = take_str(obj, &["id"]).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let account_id = take_str(obj, &["account_id", "account-id", "accountId"])
        .unwrap_or_else(|| default_account.to_string());
    let platform = take_str(obj, &["platform"]).ok_or("act: need platform")?;
    let channel = take_str(obj, &["channel"]).unwrap_or_else(|| "dev".into());
    let kind_raw = take_str(obj, &["kind"]).ok_or("act: need kind")?;
    let kind = parse_kind(&kind_raw)?;
    let text = take_str(obj, &["text"]);
    let message_id = take_str(obj, &["message_id", "message-id", "messageId"]);
    let target_user_id = take_str(obj, &["target_user_id", "target-user-id", "targetUserId"]);
    let duration_sec = take_u32(obj, &["duration_sec", "duration-sec", "durationSec"]);
    let mut job = ActJob {
        platform: platform.clone(),
        channel: channel.clone(),
        kind,
        text,
        message_id,
        target_user_id,
        duration_sec,
    };
    job.validate()?;
    Ok(ActRequest {
        id,
        account_id,
        platform: job.platform,
        channel: job.channel,
        kind: job.kind,
        text: job.text,
        message_id: job.message_id,
        target_user_id: job.target_user_id,
        duration_sec: job.duration_sec,
    })
}

fn parse_kind(raw: &str) -> Result<ActKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "send" => Ok(ActKind::Send),
        "delete" => Ok(ActKind::Delete),
        "timeout" => Ok(ActKind::Timeout),
        "ban" => Ok(ActKind::Ban),
        "unban" => Ok(ActKind::Unban),
        other => Err(format!("act: unknown kind {other}")),
    }
}

fn take_str(obj: &mut serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = obj.remove(*key) {
            if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn take_u32(obj: &mut serde_json::Map<String, Value>, keys: &[&str]) -> Option<u32> {
    for key in keys {
        if let Some(v) = obj.remove(*key) {
            if let Some(n) = v.as_u64() {
                return Some(n as u32);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<u32>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_send() {
        let acts = parse_acts(
            r#"{"kind":"send","platform":"fixture","channel":"dev","text":"hi"}"#,
            "dev",
        )
        .unwrap();
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, ActKind::Send);
        assert_eq!(acts[0].text.as_deref(), Some("hi"));
        assert_eq!(acts[0].account_id, "dev");
    }

    #[test]
    fn rejects_empty_send() {
        let err = parse_acts(
            r#"{"kind":"send","platform":"fixture","text":"  "}"#,
            "dev",
        )
        .unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }
}
