//! The original round-robin peer-selection example, retained for compatibility.

use std::{error::Error, fmt};

/// A local peer identifier, not yet a network address or cryptographic identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(pub u64);

/// Returned when a router is constructed without any peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoPeers;

impl fmt::Display for NoPeers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a router needs at least one peer")
    }
}

impl Error for NoPeers {}

/// Selects peers in round-robin order. Does not perform network I/O.
#[derive(Debug)]
pub struct Router {
    peers: Vec<PeerId>,
    cursor: usize,
}

impl Router {
    /// Creates a router. Each entry occupies one round-robin slot.
    ///
    /// # Errors
    ///
    /// Returns [`NoPeers`] if `peers` is empty.
    pub fn new(peers: Vec<PeerId>) -> Result<Self, NoPeers> {
        if peers.is_empty() {
            return Err(NoPeers);
        }

        Ok(Self { peers, cursor: 0 })
    }

    /// Returns the next peer, wrapping around after the last entry.
    pub fn next_peer(&mut self) -> PeerId {
        // Construction guarantees a nonempty list; the cursor stays in bounds.
        let peer = self.peers[self.cursor];
        self.cursor = (self.cursor + 1) % self.peers.len();
        peer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_peer_list() {
        assert!(matches!(Router::new(vec![]), Err(NoPeers)));
    }

    #[test]
    fn single_peer_can_be_selected_repeatedly() {
        let mut router = Router::new(vec![PeerId(7)]).expect("valid peers");

        for _ in 0..10 {
            assert_eq!(router.next_peer(), PeerId(7));
        }
    }
}
