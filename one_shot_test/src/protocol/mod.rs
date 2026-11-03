//! Transport-independent content identities, manifests, and chunk assembly.

mod assembly;
mod manifest;

use std::fmt;

use sha2::{Digest, Sha256};

pub use assembly::Assembler;
pub use manifest::{Manifest, Publication};

/// A SHA-256 address for chunk bytes or a canonically encoded manifest.
///
/// This proves consistency with an expected hash, not the publisher's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId([u8; 32]);

impl ContentId {
    /// Hashes exactly these bytes.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Returns the binary digest.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub(crate) fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Limits checked before chunking, storage, or allocating assembly buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes in an individual chunk.
    pub max_chunk_size: usize,
    /// Maximum bytes in a complete transfer.
    pub max_content_size: usize,
    /// Maximum entries in a manifest, including repeated chunk hashes.
    pub max_chunks: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_chunk_size: 64 * 1024,
            max_content_size: 16 * 1024 * 1024,
            max_chunks: 4096,
        }
    }
}

pub(crate) fn check_limit(
    resource: &'static str,
    actual: usize,
    limit: usize,
) -> Result<(), crate::Error> {
    if actual > limit {
        return Err(crate::Error::LimitExceeded {
            resource,
            limit,
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ContentId;

    #[test]
    fn matches_sha256_known_vector() {
        assert_eq!(
            ContentId::of(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
