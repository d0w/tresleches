//! I/O boundaries and an in-memory peer network for deterministic experiments.
//!
//! No real transport is implemented yet. A transport can resolve peer IDs to
//! addresses internally; keeping that concern separate makes routing easy to test.

use std::io;

use crate::PeerId;

mod memory;

pub use memory::MemoryNetwork;

use crate::{
    protocol::{ContentId, Manifest},
    Error,
};

/// Operations needed for replicated content delivery, independent of connection type.
///
/// The bundled implementation uses no sockets. A real adapter must enforce bounds
/// while decoding, authenticate links, and apply deadlines to every operation.
/// Successful stores acknowledge retention, not recipient delivery or disk durability.
pub trait ContentTransport {
    /// Lists known peers, including possibly disconnected peers.
    fn peers(&self) -> Vec<PeerId>;

    /// Requests verified chunk storage at a peer.
    fn put_chunk(&mut self, peer: PeerId, id: ContentId, bytes: &[u8]) -> Result<(), Error>;

    /// Requests verified manifest storage at a peer.
    fn put_manifest(
        &mut self,
        peer: PeerId,
        id: ContentId,
        manifest: &Manifest,
    ) -> Result<(), Error>;

    /// Requests a chunk. Receivers must independently verify the response.
    fn get_chunk(&mut self, peer: PeerId, id: ContentId) -> Result<Vec<u8>, Error>;

    /// Requests a manifest. Receivers must independently verify the response.
    fn get_manifest(&mut self, peer: PeerId, id: ContentId) -> Result<Manifest, Error>;
}

/// An interface for sending payloads to peers.
///
/// A future async transport may need a different interface; this synchronous
/// trait is deliberately small for the initial prototype.
pub trait Transport {
    /// Sends a payload. Success does not imply remote acknowledgement.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the payload cannot be sent.
    fn send(&mut self, peer: PeerId, payload: &[u8]) -> io::Result<()>;
}
