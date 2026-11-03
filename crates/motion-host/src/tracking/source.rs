//! Bounded, cancellable content verification at the source connector boundary.
//! This reads the caller's original encoded bytes, never a path or decoded proxy.
use super::asset::{self, Asset};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::{self, Read},
};

#[derive(Debug)]
pub enum Error {
    Asset(asset::Error),
    InvalidBudget,
    Limit { max_bytes: u64 },
    Cancelled,
    Read(io::Error),
    DigestMismatch,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asset(cause) => write!(f, "tracking source verification: {cause}"),
            Self::InvalidBudget => f.write_str("tracking source byte budget must be positive"),
            Self::Limit { max_bytes } => {
                write!(f, "tracking source exceeds byte budget {max_bytes}")
            }
            Self::Cancelled => f.write_str("tracking source verification cancelled"),
            Self::Read(cause) => write!(f, "tracking source read: {cause}"),
            Self::DigestMismatch => {
                f.write_str("tracking source content differs from imported solve")
            }
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Asset(cause) => Some(cause),
            Self::Read(cause) => Some(cause),
            _ => None,
        }
    }
}

/// Proof for the bytes read in one verification, not a permanent filesystem lease.
/// Replacement/relink/reopen must verify again. It does not verify decoded PTS,
/// dimensions, lens data, source labels, or authenticity of the external solve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifiedSource {
    sha256: [u8; 32],
    byte_len: u64,
}
impl VerifiedSource {
    pub fn sha256(self) -> [u8; 32] {
        self.sha256
    }
    pub fn byte_len(self) -> u64 {
        self.byte_len
    }
}

/// Verify a validated solve against all original source bytes up to EOF, using
/// fixed 64 KiB storage. A limit violation consumes at most max_bytes + 1 bytes.
/// The caller owns reader opening, source lifetime and request/in-flight logging.
/// Cancellation is checked before every read, including Interrupted retries; it
/// cannot interrupt an already blocked Read implementation. No timeout or retry
/// of a failed read/source is introduced. Cancelled is a normal disposition.
pub fn verify(
    asset: &Asset,
    reader: &mut impl Read,
    max_bytes: u64,
    mut cancelled: impl FnMut() -> bool,
) -> Result<VerifiedSource, Error> {
    asset.validate().map_err(Error::Asset)?;
    if max_bytes == 0 {
        return Err(Error::InvalidBudget);
    }
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        // Read one sentinel byte at the boundary to distinguish exact-size EOF.
        // Saturation handles u64::MAX without wrapping the caller's budget.
        let capacity = (max_bytes - bytes)
            .saturating_add(1)
            .min(buffer.len() as u64) as usize;
        let count = match reader.read(&mut buffer[..capacity]) {
            Ok(count) => count,
            Err(cause) if cause.kind() == io::ErrorKind::Interrupted => continue,
            Err(cause) => return Err(Error::Read(cause)),
        };
        if count == 0 {
            break;
        }
        if count as u64 > max_bytes - bytes {
            return Err(Error::Limit { max_bytes });
        }
        bytes += count as u64;
        hash.update(&buffer[..count]);
    }
    let sha256: [u8; 32] = hash.finalize().into();
    if sha256 != asset.source_sha256 {
        return Err(Error::DigestMismatch);
    }
    Ok(VerifiedSource {
        sha256,
        byte_len: bytes,
    })
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
