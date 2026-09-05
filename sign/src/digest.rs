use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Seek};
use zip::read::ZipArchive;

pub const SIGNATURE_ENTRY: &str = "signature";

pub fn package_digest<R: Read + Seek>(reader: R) -> Result<[u8; 32], String> {
    let mut archive = ZipArchive::new(reader).map_err(|err| format!("пакет не zip: {err}"))?;
    let mut entries: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|err| format!("zip: {err}"))?;
        let name = entry.name().replace('\\', "/");
        if entry.is_dir() || name == SIGNATURE_ENTRY {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|err| format!("чтение {name}: {err}"))?;
        entries.insert(name, bytes);
    }
    Ok(digest_entries(&entries))
}

pub fn package_digest_hex<R: Read + Seek>(reader: R) -> Result<String, String> {
    let digest = package_digest(reader)?;
    Ok(format!("sha256:{}", hex::encode(digest)))
}

pub fn digest_entries(entries: &BTreeMap<String, Vec<u8>>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for (path, bytes) in entries {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    hasher.finalize().into()
}

mod hex {
    pub fn encode(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn decode(hex: &str) -> Option<[u8; 32]> {
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
            let hi = from_hex(chunk[0])?;
            let lo = from_hex(chunk[1])?;
            out[i] = (hi << 4) | lo;
        }
        Some(out)
    }

    fn from_hex(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }
}

pub(crate) use hex::{decode as decode_hex, encode as encode_hex};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn sample_zip(manifest: &[u8], extra: &[(&str, &[u8])], with_signature: bool) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);
            let opts =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest", opts).unwrap();
            zip.write_all(manifest).unwrap();
            zip.start_file("module.wasm", opts).unwrap();
            zip.write_all(b"\0asm").unwrap();
            for (name, bytes) in extra {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(bytes).unwrap();
            }
            if with_signature {
                zip.start_file(SIGNATURE_ENTRY, opts).unwrap();
                zip.write_all(b"{}").unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn digest_ignores_signature_entry() {
        let manifest = b"id: com.test.demo";
        let unsigned = sample_zip(manifest, &[], false);
        let signed = sample_zip(manifest, &[], true);
        let d1 = package_digest(Cursor::new(unsigned)).unwrap();
        let d2 = package_digest(Cursor::new(signed)).unwrap();
        assert_eq!(d1, d2);
    }

    #[test]
    fn digest_changes_when_manifest_changes() {
        let a = sample_zip(b"id: com.test.a", &[], false);
        let b = sample_zip(b"id: com.test.b", &[], false);
        assert_ne!(
            package_digest(Cursor::new(a)).unwrap(),
            package_digest(Cursor::new(b)).unwrap()
        );
    }
}
