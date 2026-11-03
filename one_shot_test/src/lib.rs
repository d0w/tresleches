//! Building blocks for experimenting with decentralized routing.
//!
//! Content-addressed, replicated transfers with replaceable suppliers. The default
//! transport and provider directory are in-memory simulations, not real networking.
//! A content ID must be obtained through a trusted channel; hashes do not establish
//! publisher identity. The original round-robin API remains available.
//!
//! # Distributed transfer
//!
//! ```
//! use tresleches::{PeerId, discovery::MemoryDirectory,
//!     protocol::{Limits, Publication}, transport::MemoryNetwork,
//!     transfer::{DownloadOptions, publish, download}};
//!
//! let limits = Limits::default();
//! let mut network = MemoryNetwork::new(limits);
//! for id in 1..=3 {
//!     network.add_peer(PeerId(id), 4096)?;
//! }
//! let mut directory = MemoryDirectory::default();
//! let publication = Publication::new(b"hello distributed world", 4, limits)?;
//! let receipt = publish(&publication, 2, &mut network, &mut directory)?;
//! drop(publication);
//! network.set_online(PeerId(1), false)?;
//!
//! let received = download(receipt.content_id, DownloadOptions::default(),
//!     &mut network, &directory)?;
//! assert_eq!(received.bytes, b"hello distributed world");
//! # Ok::<(), tresleches::Error>(())
//! ```
//!
//! # Original routing example
//!
//! ```
//! use tresleches::{PeerId, Router};
//!
//! let mut router = Router::new(vec![PeerId(1), PeerId(2)])?;
//! assert_eq!(router.next_peer(), PeerId(1));
//! assert_eq!(router.next_peer(), PeerId(2));
//! # Ok::<(), tresleches::NoPeers>(())
//! ```

pub mod discovery;
mod error;
pub mod protocol;
pub mod routing;
pub mod storage;
pub mod transfer;
pub mod transport;

pub use error::Error;
pub use routing::{NoPeers, PeerId, Router};
