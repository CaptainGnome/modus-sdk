//! Host CSPRNG wrappers. WIT is only `fill`; `u64` / unit `f64` are SDK sugar.

use crate::bindings::modus::abi::random as wit;

/// CSPRNG bytes from the host. `len` must be 1..=256.
pub fn fill(len: u32) -> Result<Vec<u8>, String> {
    wit::fill(len)
}

/// Eight random bytes as little-endian `u64`.
pub fn u64() -> Result<u64, String> {
    let bytes = fill(8)?;
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| "random: expected 8 bytes".to_string())?;
    Ok(u64::from_le_bytes(arr))
}

/// Uniform float in `[0, 1)` from 53 bits of entropy.
pub fn f64() -> Result<f64, String> {
    let n = u64()?;
    Ok(((n >> 11) as f64) / ((1u64 << 53) as f64))
}
