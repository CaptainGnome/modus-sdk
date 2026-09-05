mod digest;
mod keys;
mod sign;

pub use digest::{package_digest, package_digest_hex, SIGNATURE_ENTRY};
pub use keys::{SigningKeyFile, TrustedKey, TrustedKeys};
pub use sign::{
    read_signature_file, sign_package_bytes, verify_attestation, verify_package_bytes, PackageSignature,
    SignatureFile, SignatureStatus,
};
