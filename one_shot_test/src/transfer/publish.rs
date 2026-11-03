use std::collections::BTreeSet;

use crate::{
    Error, PeerId,
    discovery::ProviderDirectory,
    protocol::{ContentId, Publication},
    transport::ContentTransport,
};

/// Summary returned only after every chunk and the manifest are replicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishReceipt {
    /// Trusted root address to share with receivers.
    pub content_id: ContentId,
    /// Number of ordered chunk positions, including duplicates.
    pub chunk_count: usize,
    /// Minimum distinct storage acknowledgments obtained per object.
    pub replicas: usize,
}

/// Distributes chunks across rotating peers, with fallback on failed stores.
///
/// Each object must be acknowledged by `replicas` distinct peers. The manifest
/// is advertised only after all chunk replication succeeds. This is not atomic:
/// a failure leaves acknowledged chunks and provider records in place. Retrying
/// is safe because storage is content-addressed and idempotent. No rollback,
/// automatic replica repair, durable storage, or recipient delivery is implied.
pub fn publish(
    publication: &Publication,
    replicas: usize,
    transport: &mut impl ContentTransport,
    directory: &mut impl ProviderDirectory,
) -> Result<PublishReceipt, Error> {
    let peers: Vec<_> = transport
        .peers()
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if replicas == 0 || replicas > peers.len() {
        return Err(Error::InvalidConfig(
            "replicas must be positive and not exceed known peer count",
        ));
    }

    for (index, (id, bytes)) in publication
        .manifest()
        .chunks()
        .iter()
        .zip(publication.chunks())
        .enumerate()
    {
        let candidates = peers
            .iter()
            .cycle()
            .skip(index % peers.len())
            .take(peers.len())
            .copied();
        replicate(*id, candidates, replicas, directory, |peer| {
            transport.put_chunk(peer, *id, bytes)
        })?;
    }
    let id = publication.id();
    replicate(id, peers.into_iter(), replicas, directory, |peer| {
        transport.put_manifest(peer, id, publication.manifest())
    })?;

    Ok(PublishReceipt {
        content_id: id,
        chunk_count: publication.chunks().len(),
        replicas,
    })
}

fn replicate(
    id: ContentId,
    candidates: impl Iterator<Item = PeerId>,
    replicas: usize,
    directory: &mut impl ProviderDirectory,
    mut store: impl FnMut(PeerId) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut stored = 0;
    for peer in candidates {
        if store(peer).is_ok() {
            directory.advertise(id, peer);
            stored += 1;
            if stored == replicas {
                return Ok(());
            }
        }
    }
    Err(Error::InsufficientReplicas {
        id,
        required: replicas,
        stored,
    })
}
