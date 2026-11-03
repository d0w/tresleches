//! Capacity-limited, volatile content storage for the prototype.

use std::collections::HashMap;

use crate::{
    Error,
    protocol::{ContentId, Limits, Manifest, check_limit},
};

/// A peer's verified chunks and manifests, retained until the process exits.
///
/// Capacity counts logical payload and canonical manifest bytes, not allocator
/// overhead. Existing objects are idempotent and are never evicted implicitly.
#[derive(Debug)]
pub struct MemoryStore {
    chunks: HashMap<ContentId, Vec<u8>>,
    manifests: HashMap<ContentId, Manifest>,
    capacity: usize,
    used: usize,
    limits: Limits,
}

impl MemoryStore {
    /// Creates an empty store with independent object and capacity limits.
    pub fn new(capacity: usize, limits: Limits) -> Self {
        Self {
            chunks: HashMap::new(),
            manifests: HashMap::new(),
            capacity,
            used: 0,
            limits,
        }
    }

    /// Verifies and stores a chunk. Acknowledgment means in-memory retention only.
    pub fn put_chunk(&mut self, id: ContentId, bytes: &[u8]) -> Result<(), Error> {
        check_limit("chunk size", bytes.len(), self.limits.max_chunk_size)?;
        // Empty transfers have zero chunks. Reject zero-byte objects so they
        // cannot consume map entries without consuming accounted payload bytes.
        if bytes.is_empty() {
            return Err(Error::InvalidConfig("stored chunks must not be empty"));
        }
        let actual = ContentId::of(bytes);
        if actual != id {
            return Err(Error::ContentMismatch {
                expected: id,
                actual,
            });
        }
        if self.chunks.contains_key(&id) {
            return Ok(());
        }
        self.reserve(bytes.len())?;
        self.chunks.insert(id, bytes.to_vec());
        self.used += bytes.len();
        Ok(())
    }

    /// Verifies and stores metadata. Chunk presence is not required on this peer.
    pub fn put_manifest(&mut self, id: ContentId, manifest: &Manifest) -> Result<(), Error> {
        manifest.validate(self.limits)?;
        let actual = manifest.id();
        if actual != id {
            return Err(Error::ContentMismatch {
                expected: id,
                actual,
            });
        }
        if self.manifests.contains_key(&id) {
            return Ok(());
        }
        self.reserve(manifest.encoded_len())?;
        self.manifests.insert(id, manifest.clone());
        self.used += manifest.encoded_len();
        Ok(())
    }

    /// Borrows chunk bytes, if present.
    pub fn chunk(&self, id: ContentId) -> Option<&[u8]> {
        self.chunks.get(&id).map(Vec::as_slice)
    }

    /// Borrows a manifest, if present.
    pub fn manifest(&self, id: ContentId) -> Option<&Manifest> {
        self.manifests.get(&id)
    }

    /// Logical bytes currently retained.
    pub fn used_bytes(&self) -> usize {
        self.used
    }

    fn reserve(&self, bytes: usize) -> Result<(), Error> {
        let available = self.capacity - self.used;
        if bytes > available {
            return Err(Error::StoreFull {
                required: bytes,
                available,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_corruption_and_enforces_capacity_without_partial_writes() -> Result<(), Error> {
        let mut store = MemoryStore::new(4, Limits::default());
        let id = ContentId::of(b"abcd");
        assert!(matches!(
            store.put_chunk(id, b"xxxx"),
            Err(Error::ContentMismatch { .. })
        ));
        assert_eq!(store.used_bytes(), 0);
        store.put_chunk(id, b"abcd")?;
        store.put_chunk(id, b"abcd")?;
        assert_eq!(store.used_bytes(), 4);
        assert!(matches!(
            store.put_chunk(ContentId::of(b"x"), b"x"),
            Err(Error::StoreFull { .. })
        ));
        assert!(store.put_chunk(ContentId::of(b""), b"").is_err());
        assert_eq!(store.used_bytes(), 4);
        Ok(())
    }

    #[test]
    fn manifest_capacity_and_identity_are_checked() -> Result<(), Error> {
        let manifest = Manifest::new(0, 4, vec![], Limits::default())?;
        let mut store = MemoryStore::new(manifest.encoded_len(), Limits::default());
        assert!(
            store
                .put_manifest(ContentId::of(b"wrong"), &manifest)
                .is_err()
        );
        assert_eq!(store.used_bytes(), 0);
        store.put_manifest(manifest.id(), &manifest)?;
        store.put_manifest(manifest.id(), &manifest)?;
        assert_eq!(store.used_bytes(), manifest.encoded_len());
        assert!(store.put_chunk(ContentId::of(b"x"), b"x").is_err());
        Ok(())
    }
}
