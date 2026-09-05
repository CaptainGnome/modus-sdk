use crate::hosts::{require_broker_url, require_https_host, HostSpec};
use crate::i18n::LocalizedString;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const ABI_VERSION: u32 = 2;
pub const SLOT_WEB: &str = "web";
pub const SLOT_PANEL: &str = "panel";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    #[serde(rename = "bus.emit")]
    BusEmit,
    #[serde(rename = "auth.token")]
    AuthToken,
    #[serde(rename = "net.http")]
    NetHttp,
    #[serde(rename = "net.ws")]
    NetWs,
    #[serde(rename = "alert.enqueue")]
    AlertEnqueue,
    #[serde(rename = "storage.kv")]
    StorageKv,
    #[serde(rename = "chat.act")]
    ChatAct,
    #[serde(rename = "ui.slot")]
    UiSlot,
    #[serde(rename = "media.cache")]
    MediaCache,
    #[serde(rename = "catalog.publish")]
    CatalogPublish,
    #[serde(rename = "history.read")]
    HistoryRead,
    #[serde(rename = "media.audio")]
    MediaAudio,
    #[serde(rename = "net.bridge")]
    NetBridge,
    #[serde(rename = "media.embed")]
    MediaEmbed,
    #[serde(rename = "rates.publish")]
    RatesPublish,
    #[serde(rename = "rates.convert")]
    RatesConvert,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Broker,
    Pkce,
    Device,
    Api,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: LocalizedString,
    pub version: String,
    pub author: String,
    pub abi: u32,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_logo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<AuthMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broker_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userinfo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embed_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<String>,
    /// Opt-in: Core UI for importing a user theme zip over web/panel assets.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_theme: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<CatalogProvide>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends: Vec<CatalogDepend>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogProvide {
    pub name: String,
    pub schema: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogDepend {
    pub platform: String,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|err| format!("manifest: {err}"))?;
        if value.get("client_secret").is_some() {
            return Err("client_secret forbidden in manifest".into());
        }
        let manifest: Manifest =
            serde_json::from_value(value).map_err(|err| format!("manifest: {err}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn i18n_keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.name.collect_key(&mut out);
        out
    }

    pub fn validate(&self) -> Result<(), String> {
        if !is_plugin_id(&self.id) {
            return Err("plugin id: reverse-DNS required (com.publisher.name)".into());
        }
        self.name
            .validate("name")
            .map_err(|err| format!("manifest: {err}"))?;
        if self.abi != ABI_VERSION {
            return Err(format!(
                "ABI {} not supported (need {ABI_VERSION})",
                self.abi
            ));
        }
        for host in &self.hosts {
            HostSpec::parse(host)?;
        }
        for host in &self.embed_hosts {
            HostSpec::parse(host)?;
        }
        if self.grants_bus_emit() {
            self.platform_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "no platform_id".to_string())?;
        }
        self.validate_auth()?;
        self.validate_slots()?;
        self.validate_user_theme()?;
        self.validate_catalog()?;
        self.validate_platform_logo()?;
        self.validate_embed()?;
        Ok(())
    }

    fn validate_user_theme(&self) -> Result<(), String> {
        if !self.user_theme {
            return Ok(());
        }
        if !self.grants_ui_slot() {
            return Err("user_theme requires capability ui.slot".into());
        }
        if !self.is_web() && !self.is_panel() {
            return Err("user_theme requires web or panel slot".into());
        }
        Ok(())
    }

    fn validate_embed(&self) -> Result<(), String> {
        let has_cap = self.grants_media_embed();
        if !self.embed_hosts.is_empty() && !has_cap {
            return Err("embed_hosts requires capability media.embed".into());
        }
        let mut seen = HashSet::new();
        for host in &self.embed_hosts {
            let spec = HostSpec::parse(host)?;
            let key = spec.canonical();
            if !seen.insert(key.clone()) {
                return Err(format!("embed_hosts: duplicate {key}"));
            }
        }
        Ok(())
    }

    fn validate_platform_logo(&self) -> Result<(), String> {
        let Some(rel) = self.platform_logo.as_deref() else {
            return Ok(());
        };
        if rel != rel.trim() || rel.is_empty() {
            return Err("platform_logo: empty path".into());
        }
        self.platform_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("platform_logo requires platform_id")?;
        if rel.contains('\\')
            || rel.contains("..")
            || rel.starts_with('/')
            || rel.starts_with("assets/")
            || rel
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err("platform_logo: path relative to assets/, no ..".into());
        }
        let ext = rel.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !matches!(ext.as_str(), "svg" | "png" | "webp" | "jpg" | "jpeg") {
            return Err("platform_logo: need svg/png/webp/jpg".into());
        }
        Ok(())
    }

    fn validate_catalog(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for item in &self.provides {
            if !seen.insert(item.name.as_str()) {
                return Err(format!("provides: duplicate {}", item.name));
            }
            match item.name.as_str() {
                "emotes" => {
                    if item.schema != "modus.emotes.v1" {
                        return Err("provides emotes: schema modus.emotes.v1".into());
                    }
                }
                other => return Err(format!("provides: unknown name {other}")),
            }
        }
        for name in &self.consumes {
            if name != "emotes" {
                return Err(format!("consumes: unknown name {name}"));
            }
        }
        for dep in &self.depends {
            if dep.platform.trim().is_empty() {
                return Err("depends: empty platform".into());
            }
        }
        Ok(())
    }

    fn validate_slots(&self) -> Result<(), String> {
        let has_cap = self.grants_ui_slot();
        if !has_cap {
            if !self.slots.is_empty() {
                return Err("slots require grant ui.slot".into());
            }
            return Ok(());
        }
        if self.slots.is_empty() {
            return Err("ui.slot requires web or panel slot".into());
        }
        let mut seen = HashSet::new();
        for slot in &self.slots {
            if !seen.insert(slot.as_str()) {
                return Err(format!("slot {slot} duplicated"));
            }
            match slot.as_str() {
                SLOT_WEB | SLOT_PANEL => {}
                other => return Err(format!("slot {other} is not supported")),
            }
        }
        Ok(())
    }

    fn validate_auth(&self) -> Result<(), String> {
        if self.auth_mode.is_none() {
            if self.auth_url.is_some() || self.token_url.is_some() || self.device_url.is_some() {
                return Err("need auth.mode".into());
            }
            return Ok(());
        }
        if !self.grants_auth() {
            return Err("auth.mode requires grant auth.token".into());
        }
        self.platform_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("no platform_id")?;
        let mode = self.auth_mode.unwrap();
        match mode {
            AuthMode::Broker => {
                require_nonempty(self.client_id.as_deref(), "missing client_id")?;
                let auth = require_nonempty(self.auth_url.as_deref(), "missing auth_url")?;
                require_https_host(auth, "auth_url", &self.hosts)?;
                let broker = require_nonempty(self.broker_url.as_deref(), "missing broker_url")?;
                require_broker_url(broker, &self.hosts)?;
                if let Some(token) = self.token_url.as_deref() {
                    require_https_host(token, "token_url", &self.hosts)?;
                }
                self.require_optional_https("userinfo_url", self.userinfo_url.as_deref())?;
            }
            AuthMode::Pkce => {
                require_nonempty(self.client_id.as_deref(), "missing client_id")?;
                let auth = require_nonempty(self.auth_url.as_deref(), "missing auth_url")?;
                let token = require_nonempty(self.token_url.as_deref(), "missing token_url")?;
                require_https_host(auth, "auth_url", &self.hosts)?;
                require_https_host(token, "token_url", &self.hosts)?;
                self.require_optional_https("userinfo_url", self.userinfo_url.as_deref())?;
            }
            AuthMode::Device => {
                require_nonempty(self.client_id.as_deref(), "missing client_id")?;
                let device = require_nonempty(self.device_url.as_deref(), "missing device_url")?;
                let token = require_nonempty(self.token_url.as_deref(), "missing token_url")?;
                require_https_host(device, "device_url", &self.hosts)?;
                require_https_host(token, "token_url", &self.hosts)?;
                self.require_optional_https("userinfo_url", self.userinfo_url.as_deref())?;
            }
            AuthMode::Api => {
                self.require_optional_https("userinfo_url", self.userinfo_url.as_deref())?;
            }
            AuthMode::Custom => {
                if self.auth_url.is_none() && self.device_url.is_none() {
                    return Err("custom: need auth_url or device_url".into());
                }
                let token = require_nonempty(self.token_url.as_deref(), "missing token_url")?;
                require_https_host(token, "token_url", &self.hosts)?;
                if let Some(auth) = self.auth_url.as_deref() {
                    require_https_host(auth, "auth_url", &self.hosts)?;
                }
                if let Some(device) = self.device_url.as_deref() {
                    require_https_host(device, "device_url", &self.hosts)?;
                }
                self.require_optional_https("userinfo_url", self.userinfo_url.as_deref())?;
            }
        }
        Ok(())
    }

    fn require_optional_https(&self, label: &str, raw: Option<&str>) -> Result<(), String> {
        if let Some(url) = raw {
            require_https_host(url, label, &self.hosts)?;
        }
        Ok(())
    }

    pub fn grants_bus_emit(&self) -> bool {
        self.capabilities.contains(&Capability::BusEmit)
    }

    pub fn grants_auth(&self) -> bool {
        self.capabilities.contains(&Capability::AuthToken)
    }

    pub fn grants_ui_slot(&self) -> bool {
        self.capabilities.contains(&Capability::UiSlot)
    }

    pub fn grants_media_cache(&self) -> bool {
        self.capabilities.contains(&Capability::MediaCache)
    }

    pub fn grants_catalog(&self) -> bool {
        self.capabilities.contains(&Capability::CatalogPublish)
    }

    pub fn grants_history(&self) -> bool {
        self.capabilities.contains(&Capability::HistoryRead)
    }

    pub fn grants_media_audio(&self) -> bool {
        self.capabilities.contains(&Capability::MediaAudio)
    }

    pub fn grants_net_bridge(&self) -> bool {
        self.capabilities.contains(&Capability::NetBridge)
    }

    pub fn grants_media_embed(&self) -> bool {
        self.capabilities.contains(&Capability::MediaEmbed)
    }

    pub fn grants_rates_publish(&self) -> bool {
        self.capabilities.contains(&Capability::RatesPublish)
    }

    pub fn grants_rates_convert(&self) -> bool {
        self.capabilities.contains(&Capability::RatesConvert)
    }

    pub fn is_web(&self) -> bool {
        self.grants_ui_slot() && self.slots.iter().any(|slot| slot == SLOT_WEB)
    }

    pub fn is_panel(&self) -> bool {
        self.grants_ui_slot() && self.slots.iter().any(|slot| slot == SLOT_PANEL)
    }

    pub fn has_ui_surface(&self) -> bool {
        self.is_web() || self.is_panel()
    }
}

pub fn is_plugin_id(id: &str) -> bool {
    if id.len() > 128 {
        return false;
    }
    let mut labels = 0usize;
    for label in id.split('.') {
        labels += 1;
        if !is_plugin_id_label(label) {
            return false;
        }
    }
    labels >= 3
}

fn is_plugin_id_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !(bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        || *bytes.last().unwrap() == b'-'
    {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn require_nonempty<'a>(value: Option<&'a str>, err: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| err.to_string())
}
