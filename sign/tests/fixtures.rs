use modus_sign::{package_digest_hex, sign_package_bytes, verify_package_bytes, SigningKeyFile, TrustedKeys, SignatureFile, SIGNATURE_ENTRY};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

fn read_sig_from_package(package: &[u8]) -> SignatureFile {
    let reader = std::io::Cursor::new(package);
    let mut archive = zip::ZipArchive::new(reader).unwrap();
    let mut entry = archive.by_name(SIGNATURE_ENTRY).unwrap();
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).unwrap();
    modus_sign::read_signature_file(&buf).unwrap()
}

fn append_signature_for_fixture(package: &[u8], signature: &SignatureFile) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    let mut out = Vec::new();
    {
        let cursor = Cursor::new(&mut out);
        let mut writer = ZipWriter::new(cursor);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let reader = std::io::Cursor::new(package);
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().to_string();
            if name == SIGNATURE_ENTRY {
                continue;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            writer.start_file(&name, opts).unwrap();
            writer.write_all(&data).unwrap();
        }
        let json = serde_json::to_vec(signature).unwrap();
        writer.start_file(SIGNATURE_ENTRY, opts).unwrap();
        writer.write_all(&json).unwrap();
        writer.finish().unwrap();
    }
    out
}

fn unsigned_package() -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;
    let mut buf = Vec::new();
    let cursor = Cursor::new(&mut buf);
    let mut zip = ZipWriter::new(cursor);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest", opts).unwrap();
    zip.write_all(b"id: com.modus.fixture\nabi: 2\nname: Fixture").unwrap();
    zip.start_file("module.wasm", opts).unwrap();
    zip.write_all(b"\0asmfixture").unwrap();
    zip.finish().unwrap();
    buf
}

#[test]
fn write_fixtures() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    fs::create_dir_all(&dir).unwrap();
    let key = SigningKeyFile::generate("fixture-test-key", "Modus Test");
    let trusted = TrustedKeys::parse(
        &serde_json::json!({
            "keys": [key.public_trusted_key().unwrap()],
            "revoked": []
        })
        .to_string(),
    )
    .unwrap();
    let unsigned = unsigned_package();
    let signed = sign_package_bytes(&unsigned, &key).unwrap();
    let mut broken_sig = read_sig_from_package(&signed);
    broken_sig.digest =
        "sha256:0000000000000000000000000000000000000000000000000000000000000000".into();
    let tampered = append_signature_for_fixture(&unsigned, &broken_sig);
    fs::write(dir.join("unsigned.mplug"), &unsigned).unwrap();
    fs::write(dir.join("signed.mplug"), &signed).unwrap();
    fs::write(dir.join("tampered.mplug"), &tampered).unwrap();
    fs::write(
        dir.join("trusted_keys.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "keys": [key.public_trusted_key().unwrap()],
            "revoked": []
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("digest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "unsigned": package_digest_hex(std::io::Cursor::new(&unsigned)).unwrap(),
            "signed": package_digest_hex(std::io::Cursor::new(&signed)).unwrap(),
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        verify_package_bytes(&signed, &trusted, Some("com.modus.fixture"))
            .unwrap()
            .status,
        modus_sign::SignatureStatus::Verified
    );
    assert_eq!(
        verify_package_bytes(&unsigned, &trusted, None).unwrap().status,
        modus_sign::SignatureStatus::Unsigned
    );
    assert_eq!(
        verify_package_bytes(&tampered, &trusted, Some("com.modus.fixture"))
            .unwrap()
            .status,
        modus_sign::SignatureStatus::Invalid
    );
}
