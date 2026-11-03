use sha2::{Digest, Sha256};

use super::{ContentId, Limits, check_limit};
use crate::Error;

// Domain separation and fixed-width integers make this independent of Rust's
// Hash implementation, native endianness, and struct layout.
const MANIFEST_DOMAIN: &[u8] = b"tresleches/manifest/v1\0";

/// Immutable metadata binding an ordered chunk list to a content ID.
///
/// The ID hashes the domain above, total length, chunk size, and chunk count
/// (each a big-endian u64), followed by the ordered 32-byte chunk hashes.
/// Empty content has no chunks. A nonempty final chunk may be shorter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    total_len: usize,
    chunk_size: usize,
    chunks: Vec<ContentId>,
}

impl Manifest {
    /// Validates metadata before constructing a manifest.
    pub fn new(
        total_len: usize,
        chunk_size: usize,
        chunks: Vec<ContentId>,
        limits: Limits,
    ) -> Result<Self, Error> {
        validate_shape(total_len, chunk_size, chunks.len(), limits)?;
        Ok(Self {
            total_len,
            chunk_size,
            chunks,
        })
    }

    /// Revalidates using a receiver's or store's own limits.
    pub fn validate(&self, limits: Limits) -> Result<(), Error> {
        validate_shape(self.total_len, self.chunk_size, self.chunks.len(), limits)
    }

    /// Computes the content address of the canonical metadata, not just the data.
    pub fn id(&self) -> ContentId {
        let mut hash = Sha256::new();
        hash.update(MANIFEST_DOMAIN);
        hash.update((self.total_len as u64).to_be_bytes());
        hash.update((self.chunk_size as u64).to_be_bytes());
        hash.update((self.chunks.len() as u64).to_be_bytes());
        for chunk in &self.chunks {
            hash.update(chunk.as_bytes());
        }
        ContentId::from_digest(hash.finalize().into())
    }

    /// Number of bytes in the reconstructed message.
    pub fn total_len(&self) -> usize {
        self.total_len
    }

    /// Maximum chunk size for this manifest.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Expected content addresses in reconstruction order.
    pub fn chunks(&self) -> &[ContentId] {
        &self.chunks
    }

    /// Exact byte length at an index, or `None` for an invalid index.
    pub fn chunk_len(&self, index: usize) -> Option<usize> {
        self.chunks.get(index)?;
        Some((self.total_len - index * self.chunk_size).min(self.chunk_size))
    }

    /// Size of canonical metadata, used for logical store accounting.
    pub fn encoded_len(&self) -> usize {
        MANIFEST_DOMAIN.len() + 24 + self.chunks.len() * 32
    }

    /// Checks a chunk's index, exact length, and hash against this manifest.
    pub fn verify_chunk(&self, index: usize, bytes: &[u8]) -> Result<(), Error> {
        let expected_len = self.chunk_len(index).ok_or(Error::InvalidChunk {
            index,
            reason: "index outside manifest",
        })?;
        if bytes.len() != expected_len {
            return Err(Error::InvalidChunk {
                index,
                reason: "length does not match manifest",
            });
        }
        if ContentId::of(bytes) != self.chunks[index] {
            return Err(Error::InvalidChunk {
                index,
                reason: "hash does not match manifest",
            });
        }
        Ok(())
    }
}

fn validate_shape(
    total_len: usize,
    chunk_size: usize,
    count: usize,
    limits: Limits,
) -> Result<(), Error> {
    if chunk_size == 0 {
        return Err(Error::InvalidConfig("chunk size must be positive"));
    }
    check_limit("chunk size", chunk_size, limits.max_chunk_size)?;
    check_limit("content size", total_len, limits.max_content_size)?;
    check_limit("chunk count", count, limits.max_chunks)?;
    if count != total_len.div_ceil(chunk_size) {
        return Err(Error::InvalidManifest(
            "chunk count does not match total length and chunk size",
        ));
    }
    Ok(())
}

/// A local publication prepared for replication to peers.
#[derive(Debug)]
pub struct Publication {
    manifest: Manifest,
    chunks: Vec<Vec<u8>>,
}

impl Publication {
    /// Splits application bytes, enforcing limits before allocating chunk copies.
    pub fn new(bytes: &[u8], chunk_size: usize, limits: Limits) -> Result<Self, Error> {
        if chunk_size == 0 {
            return Err(Error::InvalidConfig("chunk size must be positive"));
        }
        validate_shape(
            bytes.len(),
            chunk_size,
            bytes.len().div_ceil(chunk_size),
            limits,
        )?;
        let chunks: Vec<Vec<u8>> = bytes.chunks(chunk_size).map(<[u8]>::to_vec).collect();
        let ids = chunks.iter().map(|chunk| ContentId::of(chunk)).collect();
        let manifest = Manifest::new(bytes.len(), chunk_size, ids, limits)?;
        Ok(Self { manifest, chunks })
    }

    /// Metadata to distribute alongside the chunks.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Chunk bytes in manifest order.
    pub fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }

    /// Content address that a receiver must obtain through a trusted channel.
    pub fn id(&self) -> ContentId {
        self.manifest.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_boundaries_and_manifest_shape() -> Result<(), Error> {
        let publication = Publication::new(b"abcdefghij", 4, Limits::default())?;
        assert_eq!(
            publication.chunks(),
            &[b"abcd".to_vec(), b"efgh".to_vec(), b"ij".to_vec()]
        );
        assert_eq!(publication.manifest().chunk_len(2), Some(2));
        assert_eq!(publication.manifest().chunk_len(3), None);
        assert!(Manifest::new(5, 4, vec![ContentId::of(b"abcd")], Limits::default()).is_err());
        Ok(())
    }

    #[test]
    fn rejects_limits_before_chunking() {
        let limits = Limits {
            max_content_size: 2,
            ..Limits::default()
        };
        assert!(matches!(
            Publication::new(b"abc", 1, limits),
            Err(Error::LimitExceeded {
                resource: "content size",
                ..
            })
        ));
        assert!(Publication::new(b"", 0, Limits::default()).is_err());
        let limits = Limits {
            max_chunks: 1,
            ..Limits::default()
        };
        assert!(matches!(
            Publication::new(b"abc", 1, limits),
            Err(Error::LimitExceeded {
                resource: "chunk count",
                ..
            })
        ));
    }

    #[test]
    fn manifest_binds_order_and_chunking() -> Result<(), Error> {
        let first = Publication::new(b"abcd", 2, Limits::default())?;
        let reversed = Publication::new(b"cdab", 2, Limits::default())?;
        let unsplit = Publication::new(b"abcd", 4, Limits::default())?;
        assert_ne!(first.id(), reversed.id());
        assert_ne!(first.id(), unsplit.id());
        Ok(())
    }
}
