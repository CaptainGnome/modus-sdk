use base64::Engine;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustedKeysFile {
    pub keys: Vec<TrustedKey>,
    #[serde(default)]
    pub revoked: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustedKey {
    pub id: String,
    pub alg: String,
    #[serde(rename = "pub")]
    pub pub_key: String,
    pub issuer: String,
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub not_after: Option<String>,
    #[serde(default, rename = "plugin_ids")]
    pub plugin_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TrustedKeys {
    keys: Vec<TrustedKey>,
    revoked: HashSet<String>,
}

impl TrustedKeys {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|err| format!("trusted keys: {err}"))?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self, String> {
        let file: TrustedKeysFile =
            serde_json::from_str(raw).map_err(|err| format!("trusted keys JSON: {err}"))?;
        Ok(Self {
            keys: file.keys,
            revoked: file.revoked.into_iter().collect(),
        })
    }

    pub fn empty() -> Self {
        Self {
            keys: Vec::new(),
            revoked: HashSet::new(),
        }
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.keys.extend(other.keys);
        self.revoked.extend(other.revoked);
        self
    }

    pub fn find(&self, key_id: &str) -> Option<&TrustedKey> {
        if self.revoked.contains(key_id) {
            return None;
        }
        self.keys.iter().find(|k| k.id == key_id)
    }

    pub fn key_ids(&self) -> Vec<String> {
        self.keys
            .iter()
            .filter(|k| !self.revoked.contains(&k.id))
            .map(|k| k.id.clone())
            .collect()
    }

    pub fn allows_plugin(&self, key: &TrustedKey, plugin_id: &str) -> bool {
        if self.revoked.contains(&key.id) {
            return false;
        }
        if key.plugin_ids.is_empty() {
            return true;
        }
        key.plugin_ids.iter().any(|id| id == plugin_id)
    }

    pub fn verifying_key(&self, key: &TrustedKey) -> Result<VerifyingKey, String> {
        if key.alg != "ed25519" {
            return Err(format!("неизвестный alg: {}", key.alg));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(key.pub_key.trim())
            .map_err(|err| format!("pub base64: {err}"))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "pub должен быть 32 байта".to_string())?;
        VerifyingKey::from_bytes(&arr).map_err(|err| format!("pub key: {err}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SigningKeyFile {
    pub key_id: String,
    pub alg: String,
    pub private_key: String,
    #[serde(default)]
    pub issuer: Option<String>,
}

impl SigningKeyFile {
    pub fn load(path: &Path) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|err| format!("key file: {err}"))?;
        serde_json::from_str(&raw).map_err(|err| format!("key JSON: {err}"))
    }

    pub fn signing_key(&self) -> Result<SigningKey, String> {
        if self.alg != "ed25519" {
            return Err(format!("неизвестный alg: {}", self.alg));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(self.private_key.trim())
            .map_err(|err| format!("private base64: {err}"))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "private должен быть 32 байта".to_string())?;
        Ok(SigningKey::from_bytes(&arr))
    }

    pub fn generate(key_id: &str, issuer: &str) -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        Self {
            key_id: key_id.to_string(),
            alg: "ed25519".to_string(),
            private_key: base64::engine::general_purpose::STANDARD
                .encode(signing.to_bytes()),
            issuer: Some(issuer.to_string()),
        }
    }

    pub fn public_trusted_key(&self) -> Result<TrustedKey, String> {
        let signing = self.signing_key()?;
        Ok(TrustedKey {
            id: self.key_id.clone(),
            alg: self.alg.clone(),
            pub_key: base64::engine::general_purpose::STANDARD
                .encode(signing.verifying_key().to_bytes()),
            issuer: self
                .issuer
                .clone()
                .unwrap_or_else(|| self.key_id.clone()),
            not_before: None,
            not_after: None,
            plugin_ids: Vec::new(),
        })
    }
}
