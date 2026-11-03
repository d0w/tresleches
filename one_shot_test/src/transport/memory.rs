use std::collections::BTreeMap;

use super::ContentTransport;
use crate::{
    Error, PeerId,
    protocol::{ContentId, Limits, Manifest},
    storage::MemoryStore,
};

#[derive(Debug)]
struct Peer {
    online: bool,
    store: MemoryStore,
}

/// A deterministic network of independently stored, disconnectable peers.
/// All operations are direct in-process calls; this does not simulate IP routing.
#[derive(Debug)]
pub struct MemoryNetwork {
    peers: BTreeMap<PeerId, Peer>,
    limits: Limits,
}

impl MemoryNetwork {
    /// Creates an empty network using these per-object limits for new peers.
    pub fn new(limits: Limits) -> Self {
        Self {
            peers: BTreeMap::new(),
            limits,
        }
    }

    /// Adds a connected peer. Existing peer data is never overwritten.
    pub fn add_peer(&mut self, id: PeerId, capacity: usize) -> Result<(), Error> {
        if self.peers.contains_key(&id) {
            return Err(Error::InvalidConfig("peer already exists"));
        }
        self.peers.insert(
            id,
            Peer {
                online: true,
                store: MemoryStore::new(capacity, self.limits),
            },
        );
        Ok(())
    }

    /// Disconnects or reconnects a peer without changing its stored data.
    pub fn set_online(&mut self, id: PeerId, online: bool) -> Result<(), Error> {
        self.peers
            .get_mut(&id)
            .ok_or(Error::UnknownPeer(id))?
            .online = online;
        Ok(())
    }

    /// Reads logical storage usage for diagnostics, even while a peer is offline.
    pub fn stored_bytes(&self, id: PeerId) -> Result<usize, Error> {
        Ok(self
            .peers
            .get(&id)
            .ok_or(Error::UnknownPeer(id))?
            .store
            .used_bytes())
    }

    fn online_peer(&mut self, id: PeerId) -> Result<&mut Peer, Error> {
        let peer = self.peers.get_mut(&id).ok_or(Error::UnknownPeer(id))?;
        if !peer.online {
            return Err(Error::PeerOffline(id));
        }
        Ok(peer)
    }
}

impl ContentTransport for MemoryNetwork {
    fn peers(&self) -> Vec<PeerId> {
        self.peers.keys().copied().collect()
    }

    fn put_chunk(&mut self, peer: PeerId, id: ContentId, bytes: &[u8]) -> Result<(), Error> {
        self.online_peer(peer)?.store.put_chunk(id, bytes)
    }

    fn put_manifest(
        &mut self,
        peer: PeerId,
        id: ContentId,
        manifest: &Manifest,
    ) -> Result<(), Error> {
        self.online_peer(peer)?.store.put_manifest(id, manifest)
    }

    fn get_chunk(&mut self, peer: PeerId, id: ContentId) -> Result<Vec<u8>, Error> {
        self.online_peer(peer)?
            .store
            .chunk(id)
            .map(<[u8]>::to_vec)
            .ok_or(Error::NotFound { peer, id })
    }

    fn get_manifest(&mut self, peer: PeerId, id: ContentId) -> Result<Manifest, Error> {
        self.online_peer(peer)?
            .store
            .manifest(id)
            .cloned()
            .ok_or(Error::NotFound { peer, id })
    }
}
