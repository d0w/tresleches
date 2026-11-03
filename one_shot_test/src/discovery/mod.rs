//! Provider lookup boundary. The bundled directory is not a distributed hash table.

use std::collections::{BTreeSet, HashMap};

use crate::{PeerId, protocol::ContentId};

/// Advertises and locates peers claiming to retain content-addressed objects.
///
/// Records are hints, not guarantees of reachability or integrity. A future DHT
/// adapter must add network errors, expiry, record validation, and lookup bounds.
pub trait ProviderDirectory {
    /// Records a storage acknowledgment from a peer.
    fn advertise(&mut self, id: ContentId, peer: PeerId);

    /// Returns candidate providers, which may be stale or dishonest.
    fn providers(&self, id: ContentId) -> Vec<PeerId>;
}

/// Deterministic local provider index shared by simulated peers.
/// Records deliberately survive disconnects so fallback paths can be tested.
#[derive(Debug, Default)]
pub struct MemoryDirectory {
    providers: HashMap<ContentId, BTreeSet<PeerId>>,
}

impl ProviderDirectory for MemoryDirectory {
    fn advertise(&mut self, id: ContentId, peer: PeerId) {
        self.providers.entry(id).or_default().insert(peer);
    }

    fn providers(&self, id: ContentId) -> Vec<PeerId> {
        self.providers
            .get(&id)
            .map(|peers| peers.iter().copied().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertisements_are_unique_and_deterministic() {
        let mut directory = MemoryDirectory::default();
        let id = ContentId::of(b"chunk");
        directory.advertise(id, PeerId(2));
        directory.advertise(id, PeerId(1));
        directory.advertise(id, PeerId(2));
        assert_eq!(directory.providers(id), vec![PeerId(1), PeerId(2)]);
        assert!(directory.providers(ContentId::of(b"unknown")).is_empty());
    }
}
