use std::{error, fmt};

use crate::{PeerId, protocol::ContentId};

/// Failures in validation, storage, discovery, and transfer operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A local option is invalid.
    InvalidConfig(&'static str),
    /// Manifest metadata is inconsistent.
    InvalidManifest(&'static str),
    /// A bounded resource would exceed its configured limit.
    LimitExceeded {
        /// The resource being limited.
        resource: &'static str,
        /// Configured maximum.
        limit: usize,
        /// Requested amount.
        actual: usize,
    },
    /// Bytes or metadata do not match the expected content address.
    ContentMismatch {
        /// The trusted address.
        expected: ContentId,
        /// The address computed from received data.
        actual: ContentId,
    },
    /// A chunk fails index, length, or hash validation.
    InvalidChunk {
        /// Position in the manifest.
        index: usize,
        /// Validation failure.
        reason: &'static str,
    },
    /// A peer is not registered with this transport.
    UnknownPeer(PeerId),
    /// A simulated peer is disconnected.
    PeerOffline(PeerId),
    /// A peer does not have the requested object.
    NotFound {
        /// Peer that was queried.
        peer: PeerId,
        /// Requested content address.
        id: ContentId,
    },
    /// A store cannot retain additional logical payload/metadata bytes.
    StoreFull {
        /// Bytes needed by this object.
        required: usize,
        /// Remaining capacity.
        available: usize,
    },
    /// Too few distinct peers acknowledged storage of an object.
    InsufficientReplicas {
        /// Object that could not be sufficiently replicated.
        id: ContentId,
        /// Requested replica count.
        required: usize,
        /// Successful storage acknowledgments.
        stored: usize,
    },
    /// No attempted provider returned a valid manifest.
    ManifestUnavailable(ContentId),
    /// Assembly was requested before all chunks arrived.
    IncompleteTransfer {
        /// Number of chunks still missing.
        missing: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => write!(f, "invalid configuration: {reason}"),
            Self::InvalidManifest(reason) => write!(f, "invalid manifest: {reason}"),
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => {
                write!(f, "{resource} limit exceeded: {actual} > {limit}")
            }
            Self::ContentMismatch { expected, actual } => {
                write!(f, "content mismatch: expected {expected}, got {actual}")
            }
            Self::InvalidChunk { index, reason } => write!(f, "invalid chunk {index}: {reason}"),
            Self::UnknownPeer(peer) => write!(f, "unknown peer {}", peer.0),
            Self::PeerOffline(peer) => write!(f, "peer {} is offline", peer.0),
            Self::NotFound { peer, id } => write!(f, "peer {} does not hold {id}", peer.0),
            Self::StoreFull {
                required,
                available,
            } => {
                write!(f, "store full: need {required} bytes, have {available}")
            }
            Self::InsufficientReplicas {
                id,
                required,
                stored,
            } => {
                write!(
                    f,
                    "insufficient replicas for {id}: need {required}, stored {stored}"
                )
            }
            Self::ManifestUnavailable(id) => write!(f, "no valid manifest available for {id}"),
            Self::IncompleteTransfer { missing } => {
                write!(f, "transfer has {missing} missing chunks")
            }
        }
    }
}

impl error::Error for Error {}
