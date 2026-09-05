use crate::digest::{decode_hex, encode_hex, package_digest, SIGNATURE_ENTRY};
use crate::keys::{SigningKeyFile, TrustedKeys};
use base64::Engine;
use ed25519_dalek::{Signature, Signer, Verifier};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatus {
    Verified,
    Unsigned,
    Invalid,
    Untrusted,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignatureFile {
    pub v: u32,
    pub alg: String,
    pub key_id: String,
    pub digest: String,
    pub sig: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PackageSignature {
    pub status: SignatureStatus,
    pub key_id: Option<String>,
    pub issuer: Option<String>,
    pub digest: [u8; 32],
    pub file: Option<SignatureFile>,
}

pub fn read_signature_file(bytes: &[u8]) -> Result<SignatureFile, String> {
    serde_json::from_slice(bytes).map_err(|err| format!("signature JSON: {err}"))
}

pub fn verify_package_bytes(
    package: &[u8],
    trusted: &TrustedKeys,
    plugin_id: Option<&str>,
) -> Result<PackageSignature, String> {
    let actual = package_digest(Cursor::new(package))?;
    let Some(file) = read_signature_from_zip(package)? else {
        return Ok(PackageSignature {
            status: SignatureStatus::Unsigned,
            key_id: None,
            issuer: None,
            digest: actual,
            file: None,
        });
    };
    verify_with_file(actual, file, trusted, plugin_id)
}

pub fn verify_attestation(
    digest_hex: &str,
    key_id: &str,
    sig_b64: &str,
    trusted: &TrustedKeys,
    plugin_id: &str,
) -> Result<PackageSignature, String> {
    let digest = parse_digest_field(digest_hex)?;
    let file = SignatureFile {
        v: 1,
        alg: "ed25519".to_string(),
        key_id: key_id.to_string(),
        digest: digest_hex.to_string(),
        sig: sig_b64.to_string(),
        license: None,
    };
    verify_with_file(digest, file, trusted, Some(plugin_id))
}

fn verify_with_file(
    actual: [u8; 32],
    file: SignatureFile,
    trusted: &TrustedKeys,
    plugin_id: Option<&str>,
) -> Result<PackageSignature, String> {
    if file.v != 1 || file.alg != "ed25519" {
        return Ok(invalid(actual, Some(file)));
    }
    let expected = match parse_digest_field(&file.digest) {
        Ok(d) => d,
        Err(_) => return Ok(invalid(actual, Some(file))),
    };
    if expected != actual {
        return Ok(invalid(actual, Some(file)));
    }
    let Some(key) = trusted.find(&file.key_id) else {
        return Ok(PackageSignature {
            status: SignatureStatus::Untrusted,
            key_id: Some(file.key_id.clone()),
            issuer: None,
            digest: actual,
            file: Some(file),
        });
    };
    if let Some(plugin_id) = plugin_id {
        if !trusted.allows_plugin(key, plugin_id) {
            return Ok(PackageSignature {
                status: SignatureStatus::Untrusted,
                key_id: Some(file.key_id.clone()),
                issuer: Some(key.issuer.clone()),
                digest: actual,
                file: Some(file),
            });
        }
    }
    let sig_bytes = match base64::engine::general_purpose::STANDARD.decode(file.sig.trim()) {
        Ok(b) => b,
        Err(_) => return Ok(invalid(actual, Some(file))),
    };
    let sig = match Signature::from_slice(&sig_bytes) {
        Ok(s) => s,
        Err(_) => return Ok(invalid(actual, Some(file))),
    };
    let verifying = match trusted.verifying_key(key) {
        Ok(v) => v,
        Err(_) => return Ok(invalid(actual, Some(file))),
    };
    if verifying.verify(&actual, &sig).is_err() {
        return Ok(invalid(actual, Some(file)));
    }
    Ok(PackageSignature {
        status: SignatureStatus::Verified,
        key_id: Some(file.key_id.clone()),
        issuer: Some(key.issuer.clone()),
        digest: actual,
        file: Some(file),
    })
}

fn invalid(digest: [u8; 32], file: Option<SignatureFile>) -> PackageSignature {
    PackageSignature {
        status: SignatureStatus::Invalid,
        key_id: file.as_ref().map(|f| f.key_id.clone()),
        issuer: None,
        digest,
        file,
    }
}

fn parse_digest_field(raw: &str) -> Result<[u8; 32], String> {
    let hex = raw
        .strip_prefix("sha256:")
        .ok_or_else(|| "digest: нужен префикс sha256:".to_string())?;
    decode_hex(hex).ok_or_else(|| "digest: плохой hex".to_string())
}

fn read_signature_from_zip(package: &[u8]) -> Result<Option<SignatureFile>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(package)).map_err(|err| format!("zip: {err}"))?;
    let mut file = match archive.by_name(SIGNATURE_ENTRY) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|err| format!("чтение signature: {err}"))?;
    Ok(Some(read_signature_file(&buf)?))
}

pub fn sign_package_bytes(package: &[u8], key: &SigningKeyFile) -> Result<Vec<u8>, String> {
    let digest = package_digest(Cursor::new(package))?;
    let signing = key.signing_key()?;
    let sig = signing.sign(&digest);
    let signature = SignatureFile {
        v: 1,
        alg: "ed25519".to_string(),
        key_id: key.key_id.clone(),
        digest: format!("sha256:{}", encode_hex(digest)),
        sig: base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()),
        license: None,
    };
    append_signature(package, &signature)
}

fn append_signature(package: &[u8], signature: &SignatureFile) -> Result<Vec<u8>, String> {
    let mut reader =
        zip::ZipArchive::new(Cursor::new(package)).map_err(|err| format!("zip: {err}"))?;
    let mut out = Vec::new();
    {
        let cursor = Cursor::new(&mut out);
        let mut writer = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for i in 0..reader.len() {
            let mut entry = reader.by_index(i).map_err(|err| format!("zip: {err}"))?;
            let name = entry.name().replace('\\', "/");
            if name == SIGNATURE_ENTRY {
                continue;
            }
            let compression = entry.compression();
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|err| format!("чтение {name}: {err}"))?;
            let method = match compression {
                zip::CompressionMethod::Stored => zip::CompressionMethod::Stored,
                _ => zip::CompressionMethod::Deflated,
            };
            let opts = opts.compression_method(method);
            writer
                .start_file(&name, opts)
                .map_err(|err| format!("zip write {name}: {err}"))?;
            writer
                .write_all(&bytes)
                .map_err(|err| format!("zip body {name}: {err}"))?;
        }
        let json = serde_json::to_vec(signature).map_err(|err| err.to_string())?;
        writer
            .start_file(SIGNATURE_ENTRY, opts)
            .map_err(|err| format!("zip signature: {err}"))?;
        writer
            .write_all(&json)
            .map_err(|err| format!("zip signature body: {err}"))?;
        writer.finish().map_err(|err| format!("zip finish: {err}"))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn unsigned_package() -> Vec<u8> {
        let mut buf = Vec::new();
        let cursor = Cursor::new(&mut buf);
        let mut zip = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest", opts).unwrap();
        zip.write_all(b"id: com.test.demo\nabi: 2").unwrap();
        zip.start_file("module.wasm", opts).unwrap();
        zip.write_all(b"\0asm").unwrap();
        zip.finish().unwrap();
        buf
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let key = SigningKeyFile::generate("test-key", "Test");
        let trusted = TrustedKeys::parse(&serde_json::json!({
            "keys": [key.public_trusted_key().unwrap()],
            "revoked": []
        })
        .to_string())
        .unwrap();
        let signed = sign_package_bytes(&unsigned_package(), &key).unwrap();
        let meta = verify_package_bytes(&signed, &trusted, Some("com.test.demo")).unwrap();
        assert_eq!(meta.status, SignatureStatus::Verified);
        assert_eq!(meta.key_id.as_deref(), Some("test-key"));
    }

    #[test]
    fn tampered_package_is_invalid() {
        let key = SigningKeyFile::generate("test-key", "Test");
        let trusted = TrustedKeys::parse(&serde_json::json!({
            "keys": [key.public_trusted_key().unwrap()],
            "revoked": []
        })
        .to_string())
        .unwrap();
        let signed = sign_package_bytes(&unsigned_package(), &key).unwrap();
        let mut tampered = unsigned_package();
        tampered.extend_from_slice(b"tamper");
        let resigned = sign_package_bytes(&tampered, &key).unwrap();
        let meta = verify_package_bytes(&resigned, &trusted, Some("com.test.demo")).unwrap();
        assert_eq!(meta.status, SignatureStatus::Verified);
        let mut bad = signed;
        if let Some(file) = read_signature_from_zip(&bad).unwrap() {
            let mut broken = file;
            broken.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
            bad = append_signature(&unsigned_package(), &broken).unwrap();
        }
        let meta = verify_package_bytes(&bad, &trusted, Some("com.test.demo")).unwrap();
        assert_eq!(meta.status, SignatureStatus::Invalid);
    }

    #[test]
    fn unsigned_package_status() {
        let trusted = TrustedKeys::empty();
        let meta = verify_package_bytes(&unsigned_package(), &trusted, None).unwrap();
        assert_eq!(meta.status, SignatureStatus::Unsigned);
    }
}
