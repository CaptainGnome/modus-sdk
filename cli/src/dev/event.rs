use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_BODY_BYTES: usize = 64 * 1024;

const CANON_KIND_NAMES: &[&str] = &[
    "message",
    "donation",
    "sub",
    "follow",
    "raid",
    "viewer_count",
    "reward",
    "moderation",
    "system",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub plugin_id: String,
    pub platform: String,
    pub channel: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventKind {
    Message,
    Donation,
    Sub,
    Follow,
    Raid,
    ViewerCount,
    Reward,
    Moderation,
    System,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Fragment {
    Text {
        text: String,
    },
    Emote {
        id: String,
        alt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Mention {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
    },
    Url {
        href: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Money {
    pub amount: f64,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModAction {
    Delete,
    Timeout,
    Ban,
    Unban,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Payload {
    #[serde(rename_all = "camelCase")]
    Message {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
        fragments: Vec<Fragment>,
        #[serde(default, alias = "name_color", skip_serializing_if = "Option::is_none")]
        name_color: Option<String>,
        #[serde(default, alias = "message_id", skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        rewarded: bool,
    },
    #[serde(rename_all = "camelCase")]
    Donation {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
        money: Money,
        #[serde(default)]
        fragments: Vec<Fragment>,
    },
    #[serde(rename_all = "camelCase")]
    Sub {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
        months: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tier: Option<String>,
        gifted: bool,
        #[serde(default, alias = "gifter_id", skip_serializing_if = "Option::is_none")]
        gifter_id: Option<String>,
        #[serde(default, alias = "gifter_name", skip_serializing_if = "Option::is_none")]
        gifter_name: Option<String>,
        #[serde(default)]
        fragments: Vec<Fragment>,
    },
    #[serde(rename_all = "camelCase")]
    Follow {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
    },
    #[serde(rename_all = "camelCase")]
    Raid {
        #[serde(alias = "from_user_id")]
        from_user_id: String,
        #[serde(alias = "from_display_name")]
        from_display_name: String,
        viewers: u32,
    },
    #[serde(rename_all = "camelCase")]
    ViewerCount {
        count: u32,
    },
    #[serde(rename_all = "camelCase")]
    Reward {
        #[serde(alias = "user_id")]
        user_id: String,
        #[serde(alias = "display_name")]
        display_name: String,
        #[serde(alias = "reward_id")]
        reward_id: String,
        title: String,
        cost: u32,
        #[serde(default)]
        fragments: Vec<Fragment>,
        #[serde(default, alias = "image_url", skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Moderation {
        action: ModAction,
        #[serde(alias = "target_user_id")]
        target_user_id: String,
        #[serde(alias = "target_display_name")]
        target_display_name: String,
        #[serde(default, alias = "moderator_id", skip_serializing_if = "Option::is_none")]
        moderator_id: Option<String>,
        #[serde(default, alias = "moderator_name", skip_serializing_if = "Option::is_none")]
        moderator_name: Option<String>,
        #[serde(default, alias = "message_id", skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
        #[serde(default, alias = "duration_sec", skip_serializing_if = "Option::is_none")]
        duration_sec: Option<u32>,
    },
    System(SystemEvent),
    Custom {
        kind: String,
        #[serde(default)]
        fields: Vec<(String, String)>,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SystemCode {
    PluginDisabled,
    PluginCrashed,
    PluginQuarantined,
    PluginRollback,
    PluginReconnecting,
    PluginLoadFailed,
    PluginRemoved,
    AuthConnected,
    AuthDisconnected,
    AuthRevoked,
    AuthLoginFailed,
    NetworkResume,
    WsClosed,
    Unknown,
}

impl SystemCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PluginDisabled => "plugin-disabled",
            Self::PluginCrashed => "plugin-crashed",
            Self::PluginQuarantined => "plugin-quarantined",
            Self::PluginRollback => "plugin-rollback",
            Self::PluginReconnecting => "plugin-reconnecting",
            Self::PluginLoadFailed => "plugin-load-failed",
            Self::PluginRemoved => "plugin-removed",
            Self::AuthConnected => "auth-connected",
            Self::AuthDisconnected => "auth-disconnected",
            Self::AuthRevoked => "auth-revoked",
            Self::AuthLoginFailed => "auth-login-failed",
            Self::NetworkResume => "network-resume",
            Self::WsClosed => "ws-closed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEvent {
    pub code: SystemCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl<'de> Deserialize<'de> for SystemEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            #[serde(default)]
            code: Option<SystemCode>,
            #[serde(default, alias = "plugin_id")]
            plugin_id: Option<String>,
            #[serde(default, alias = "account_id")]
            account_id: Option<String>,
            #[serde(default)]
            platform: Option<String>,
            #[serde(default)]
            detail: Option<String>,
            #[serde(default)]
            text: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        match raw.code {
            Some(code) => Ok(Self {
                code,
                plugin_id: raw.plugin_id,
                account_id: raw.account_id,
                platform: raw.platform,
                detail: raw.detail,
            }),
            None => Ok(Self {
                code: SystemCode::Unknown,
                plugin_id: None,
                account_id: None,
                platform: None,
                detail: raw.text.or(raw.detail),
            }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilterFlags {
    pub hide_chat: bool,
    pub skip_alert: bool,
    pub highlight: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: Uuid,
    pub ts: i64,
    pub source: Source,
    pub kind: EventKind,
    pub payload: Payload,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opaque: Option<serde_json::Value>,
    pub flags: FilterFlags,
}

#[derive(Clone, Debug)]
pub struct EmitRequest {
    pub source: Source,
    pub kind: EventKind,
    pub payload: Payload,
    pub opaque: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropReason {
    TooLarge,
    KindMismatch,
}

impl std::fmt::Display for DropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DropReason::TooLarge => write!(f, "TooLarge"),
            DropReason::KindMismatch => write!(f, "invalid kind"),
        }
    }
}

impl Event {
    pub fn stamp(req: EmitRequest) -> Result<Self, DropReason> {
        if kind_of(&req.payload) != req.kind {
            return Err(DropReason::KindMismatch);
        }
        let event = Self {
            id: Uuid::now_v7(),
            ts: now_ms(),
            source: req.source,
            kind: req.kind,
            payload: req.payload,
            opaque: req.opaque,
            flags: FilterFlags::default(),
        };
        if event.body_size() > MAX_BODY_BYTES {
            return Err(DropReason::TooLarge);
        }
        Ok(event)
    }

    fn body_size(&self) -> usize {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX)
    }
}

pub fn kind_of(payload: &Payload) -> EventKind {
    match payload {
        Payload::Message { .. } => EventKind::Message,
        Payload::Donation { .. } => EventKind::Donation,
        Payload::Sub { .. } => EventKind::Sub,
        Payload::Follow { .. } => EventKind::Follow,
        Payload::Raid { .. } => EventKind::Raid,
        Payload::ViewerCount { .. } => EventKind::ViewerCount,
        Payload::Reward { .. } => EventKind::Reward,
        Payload::Moderation { .. } => EventKind::Moderation,
        Payload::System(_) => EventKind::System,
        Payload::Custom { .. } => EventKind::Custom,
    }
}

pub fn kind_as_str(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Message => "message",
        EventKind::Donation => "donation",
        EventKind::Sub => "sub",
        EventKind::Follow => "follow",
        EventKind::Raid => "raid",
        EventKind::ViewerCount => "viewer_count",
        EventKind::Reward => "reward",
        EventKind::Moderation => "moderation",
        EventKind::System => "system",
        EventKind::Custom => "custom",
    }
}

pub fn plugin_emit_request(
    plugin_id: String,
    platform_id: Option<String>,
    channel: String,
    payload: Payload,
    opaque: Option<serde_json::Value>,
) -> Result<EmitRequest, String> {
    match &payload {
        Payload::System(_) => return Err("system is Core-only".into()),
        Payload::Custom { kind, .. } => {
            if CANON_KIND_NAMES.contains(&kind.as_str()) {
                return Err("custom cannot mask canon".into());
            }
        }
        Payload::Message { .. }
        | Payload::Donation { .. }
        | Payload::Sub { .. }
        | Payload::Follow { .. }
        | Payload::Raid { .. }
        | Payload::ViewerCount { .. }
        | Payload::Reward { .. }
        | Payload::Moderation { .. } => {
            if platform_id.as_deref().filter(|s| !s.is_empty()).is_none() {
                return Err("no platform_id".into());
            }
        }
    }

    let platform = match &payload {
        Payload::Custom { .. } => platform_id.filter(|s| !s.is_empty()).unwrap_or_default(),
        _ => platform_id.unwrap_or_default(),
    };

    Ok(EmitRequest {
        kind: kind_of(&payload),
        payload,
        opaque,
        source: Source {
            plugin_id,
            platform,
            channel,
        },
    })
}

pub fn parse_opaque(raw: Option<String>) -> Result<Option<serde_json::Value>, String> {
    match raw {
        None => Ok(None),
        Some(text) if text.trim().is_empty() => Ok(None),
        Some(text) => serde_json::from_str(&text).map_err(|_| "opaque is not JSON".to_string()),
    }
}

pub fn sanitize_name_color(raw: Option<String>) -> Option<String> {
    let value = raw?.trim().to_string();
    if value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
    {
        Some(value)
    } else {
        None
    }
}

pub fn payload_text(payload: &Payload) -> String {
    match payload {
        Payload::Message { fragments, .. } => fragments_text(fragments),
        Payload::Donation {
            fragments, money, ..
        } => {
            let text = fragments_text(fragments);
            if text.is_empty() {
                format!("{} {}", money.amount, money.currency)
            } else {
                format!("{} {} {text}", money.amount, money.currency)
            }
        }
        Payload::Sub { fragments, .. } => fragments_text(fragments),
        Payload::Follow { display_name, .. } => display_name.clone(),
        Payload::Raid {
            from_display_name, ..
        } => from_display_name.clone(),
        Payload::ViewerCount { count } => count.to_string(),
        Payload::Reward { title, fragments, .. } => {
            let text = fragments_text(fragments);
            if text.is_empty() {
                title.clone()
            } else {
                format!("{title} {text}")
            }
        }
        Payload::Moderation {
            target_display_name,
            ..
        } => target_display_name.clone(),
        Payload::System(ev) => ev
            .detail
            .clone()
            .unwrap_or_else(|| ev.code.as_str().to_string()),
        Payload::Custom { kind, .. } => kind.clone(),
    }
}

fn fragments_text(fragments: &[Fragment]) -> String {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            Fragment::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_cannot_mask_canon() {
        let err = plugin_emit_request(
            "com.example.emit".into(),
            Some("plat".into()),
            "dev".into(),
            Payload::Custom {
                kind: "message".into(),
                fields: vec![],
            },
            None,
        )
        .unwrap_err();
        assert_eq!(err, "custom cannot mask canon");
    }

    #[test]
    fn system_only_core() {
        let err = plugin_emit_request(
            "com.example.emit".into(),
            Some("plat".into()),
            "dev".into(),
            Payload::System(SystemEvent {
                code: SystemCode::Unknown,
                plugin_id: None,
                account_id: None,
                platform: None,
                detail: Some("hi".into()),
            }),
            None,
        )
        .unwrap_err();
        assert_eq!(err, "system is Core-only");
    }

    #[test]
    fn canon_needs_platform() {
        let err = plugin_emit_request(
            "com.example.emit".into(),
            None,
            "dev".into(),
            Payload::Message {
                user_id: "1".into(),
                display_name: "n".into(),
                fragments: vec![],
                name_color: None,
                message_id: None,
                rewarded: false,
            },
            None,
        )
        .unwrap_err();
        assert_eq!(err, "no platform_id");
    }

    #[test]
    fn custom_cannot_mask_reward() {
        let err = plugin_emit_request(
            "com.example.emit".into(),
            Some("plat".into()),
            "dev".into(),
            Payload::Custom {
                kind: "reward".into(),
                fields: vec![],
            },
            None,
        )
        .unwrap_err();
        assert_eq!(err, "custom cannot mask canon");
    }
}
