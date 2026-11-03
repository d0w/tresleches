//! End-to-end behavior through public APIs; no sockets, sleeps, or shared state.

use tresleches::{
    Error, PeerId,
    discovery::{MemoryDirectory, ProviderDirectory},
    protocol::{ContentId, Limits, Manifest, Publication},
    transfer::{DownloadOptions, DownloadSession, download, publish},
    transport::{ContentTransport, MemoryNetwork},
};

fn network(peers: u64, capacity: usize) -> Result<MemoryNetwork, Error> {
    let mut network = MemoryNetwork::new(Limits::default());
    for id in 1..=peers {
        network.add_peer(PeerId(id), capacity)?;
    }
    Ok(network)
}

#[test]
fn reconstructs_from_multiple_peers_after_publisher_disconnects() -> Result<(), Error> {
    let mut network = network(4, 4096)?;
    let mut directory = MemoryDirectory::default();
    let original = b"abcdefghijklmnopqrstuvwxyz";
    let publication = Publication::new(original, 4, Limits::default())?;
    let receipt = publish(&publication, 2, &mut network, &mut directory)?;
    let id = receipt.content_id;
    for chunk in publication.manifest().chunks() {
        assert_eq!(directory.providers(*chunk).len(), 2);
    }
    drop(publication);
    network.set_online(PeerId(1), false)?;

    let result = download(id, DownloadOptions::default(), &mut network, &directory)?;
    assert_eq!(result.bytes, original);
    assert_eq!(result.content_id, id);
    assert!(result.providers.len() > 1);
    assert!(!result.providers.contains(&PeerId(1)));
    // Disconnect does not remove stale provider advertisements.
    assert!(directory.providers(id).contains(&PeerId(1)));
    Ok(())
}

#[test]
fn retains_progress_and_switches_suppliers_in_later_round() -> Result<(), Error> {
    let mut network = network(4, 4096)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefghijklmnop", 4, Limits::default())?;
    publish(&publication, 1, &mut network, &mut directory)?;
    network.set_online(PeerId(2), false)?;
    let mut session = DownloadSession::open(
        publication.id(),
        DownloadOptions::default(),
        &mut network,
        &directory,
    )?;

    let first = session.fetch_missing(&mut network, &directory);
    assert_eq!((first.received, first.total, first.attempts), (3, 4, 4));
    assert!(!session.is_complete());
    assert_eq!(session.missing_indices().collect::<Vec<_>>(), vec![1]);

    // A replacement receives the unavailable chunk from an independent source.
    let chunk = publication.manifest().chunks()[1];
    network.put_chunk(PeerId(3), chunk, &publication.chunks()[1])?;
    directory.advertise(chunk, PeerId(3));
    network.set_online(PeerId(1), false)?;
    let second = session.fetch_missing(&mut network, &directory);
    assert_eq!((second.received, second.total, second.attempts), (4, 4, 1));
    assert!(session.is_complete());
    assert_eq!(session.fetch_missing(&mut network, &directory).attempts, 0);
    assert_eq!(session.finish()?.bytes, b"abcdefghijklmnop");
    Ok(())
}

#[test]
fn unavailable_chunks_never_produce_partial_success() -> Result<(), Error> {
    let mut network = network(2, 4096)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    publish(&publication, 1, &mut network, &mut directory)?;
    let mut session = DownloadSession::open(
        publication.id(),
        DownloadOptions::default(),
        &mut network,
        &directory,
    )?;
    network.set_online(PeerId(1), false)?;
    network.set_online(PeerId(2), false)?;
    let progress = session.fetch_missing(&mut network, &directory);
    assert_eq!(progress.received, 0);
    assert_eq!(progress.attempts, 2);
    assert_eq!(
        session.finish(),
        Err(Error::IncompleteTransfer { missing: 2 })
    );
    assert!(matches!(
        download(
            publication.id(),
            DownloadOptions::default(),
            &mut network,
            &directory
        ),
        Err(Error::ManifestUnavailable(_))
    ));
    Ok(())
}

#[test]
fn empty_and_repeated_chunk_content_round_trip() -> Result<(), Error> {
    for original in [b"".as_slice(), b"aaaaaaaaaa".as_slice()] {
        let mut network = network(3, 4096)?;
        let mut directory = MemoryDirectory::default();
        let publication = Publication::new(original, 2, Limits::default())?;
        publish(&publication, 2, &mut network, &mut directory)?;
        network.set_online(PeerId(1), false)?;
        assert_eq!(
            download(
                publication.id(),
                DownloadOptions::default(),
                &mut network,
                &directory
            )?
            .bytes,
            original
        );
    }
    Ok(())
}

#[test]
fn publishing_skips_offline_and_full_peers() -> Result<(), Error> {
    let mut network = network(1, 0)?;
    network.add_peer(PeerId(2), 4096)?;
    network.add_peer(PeerId(3), 4096)?;
    network.add_peer(PeerId(4), 4096)?;
    network.set_online(PeerId(2), false)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    publish(&publication, 2, &mut network, &mut directory)?;
    for id in publication
        .manifest()
        .chunks()
        .iter()
        .copied()
        .chain([publication.id()])
    {
        assert_eq!(directory.providers(id), vec![PeerId(3), PeerId(4)]);
    }
    assert_eq!(network.stored_bytes(PeerId(1))?, 0);
    assert_eq!(
        download(
            publication.id(),
            DownloadOptions::default(),
            &mut network,
            &directory
        )?
        .bytes,
        b"abcdefgh"
    );
    Ok(())
}

#[test]
fn failed_publication_is_explicit_and_can_be_retried() -> Result<(), Error> {
    let mut network = network(2, 4096)?;
    network.set_online(PeerId(2), false)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    assert!(matches!(
        publish(&publication, 2, &mut network, &mut directory),
        Err(Error::InsufficientReplicas {
            required: 2,
            stored: 1,
            ..
        })
    ));
    assert!(directory.providers(publication.id()).is_empty());
    assert_eq!(
        directory.providers(publication.manifest().chunks()[0]),
        vec![PeerId(1)]
    );

    network.set_online(PeerId(2), true)?;
    publish(&publication, 2, &mut network, &mut directory)?;
    let used = network.stored_bytes(PeerId(1))?;
    publish(&publication, 2, &mut network, &mut directory)?;
    assert_eq!(network.stored_bytes(PeerId(1))?, used);
    assert_eq!(
        download(
            publication.id(),
            DownloadOptions::default(),
            &mut network,
            &directory
        )?
        .bytes,
        b"abcdefgh"
    );
    Ok(())
}

#[test]
fn checks_replication_options_and_does_not_overwrite_peers() -> Result<(), Error> {
    let mut network = network(1, 4096)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abc", 4, Limits::default())?;
    assert!(publish(&publication, 0, &mut network, &mut directory).is_err());
    assert!(publish(&publication, 2, &mut network, &mut directory).is_err());
    publish(&publication, 1, &mut network, &mut directory)?;
    let used = network.stored_bytes(PeerId(1))?;
    assert!(network.add_peer(PeerId(1), 0).is_err());
    assert_eq!(network.stored_bytes(PeerId(1))?, used);
    Ok(())
}

#[test]
fn receiver_applies_its_own_limits() -> Result<(), Error> {
    let mut network = network(1, 4096)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    publish(&publication, 1, &mut network, &mut directory)?;
    let options = DownloadOptions {
        limits: Limits {
            max_content_size: 4,
            ..Limits::default()
        },
        ..DownloadOptions::default()
    };
    assert!(matches!(
        DownloadSession::open(publication.id(), options, &mut network, &directory),
        Err(Error::ManifestUnavailable(_))
    ));
    let options = DownloadOptions {
        max_provider_attempts: 0,
        ..DownloadOptions::default()
    };
    assert!(matches!(
        DownloadSession::open(publication.id(), options, &mut network, &directory),
        Err(Error::InvalidConfig(_))
    ));
    Ok(())
}

/// Injects Byzantine replies after storage checks, ensuring the receiver itself
/// validates responses rather than relying on an honest MemoryStore.
struct FaultyNetwork {
    inner: MemoryNetwork,
    corrupt_chunks: bool,
    wrong_manifest: bool,
    chunk_requests: Vec<(PeerId, ContentId)>,
}

impl ContentTransport for FaultyNetwork {
    fn peers(&self) -> Vec<PeerId> {
        self.inner.peers()
    }

    fn put_chunk(&mut self, peer: PeerId, id: ContentId, bytes: &[u8]) -> Result<(), Error> {
        self.inner.put_chunk(peer, id, bytes)
    }

    fn put_manifest(
        &mut self,
        peer: PeerId,
        id: ContentId,
        manifest: &Manifest,
    ) -> Result<(), Error> {
        self.inner.put_manifest(peer, id, manifest)
    }

    fn get_chunk(&mut self, peer: PeerId, id: ContentId) -> Result<Vec<u8>, Error> {
        self.chunk_requests.push((peer, id));
        let mut bytes = self.inner.get_chunk(peer, id)?;
        if peer == PeerId(1) && self.corrupt_chunks {
            bytes[0] ^= 0xff;
        }
        Ok(bytes)
    }

    fn get_manifest(&mut self, peer: PeerId, id: ContentId) -> Result<Manifest, Error> {
        if peer == PeerId(1) && self.wrong_manifest {
            return Ok(
                Publication::new(b"unrelated content", 4, Limits::default())?
                    .manifest()
                    .clone(),
            );
        }
        self.inner.get_manifest(peer, id)
    }
}

#[test]
fn rejects_dishonest_manifests_and_chunks_then_uses_other_providers() -> Result<(), Error> {
    let mut network = FaultyNetwork {
        inner: network(3, 4096)?,
        corrupt_chunks: true,
        wrong_manifest: true,
        chunk_requests: vec![],
    };
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefghijkl", 4, Limits::default())?;
    publish(&publication, 2, &mut network, &mut directory)?;
    let result = download(
        publication.id(),
        DownloadOptions::default(),
        &mut network,
        &directory,
    )?;
    assert_eq!(result.bytes, b"abcdefghijkl");
    assert!(!result.providers.contains(&PeerId(1)));
    assert!(
        network
            .chunk_requests
            .iter()
            .any(|(peer, _)| *peer == PeerId(1))
    );
    Ok(())
}

#[test]
fn provider_attempts_are_bounded_and_completed_chunks_are_not_refetched() -> Result<(), Error> {
    let mut network = FaultyNetwork {
        inner: network(2, 4096)?,
        corrupt_chunks: true,
        wrong_manifest: false,
        chunk_requests: vec![],
    };
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    publish(&publication, 2, &mut network, &mut directory)?;
    let options = DownloadOptions {
        max_provider_attempts: 1,
        ..DownloadOptions::default()
    };
    let mut session = DownloadSession::open(publication.id(), options, &mut network, &directory)?;
    let first = session.fetch_missing(&mut network, &directory);
    assert_eq!((first.received, first.attempts), (0, 2));
    // The dishonest peer remains dishonest. The next bounded round rotates to
    // the second supplier rather than repeating the first supplier forever.
    assert_eq!(session.fetch_missing(&mut network, &directory).received, 2);
    assert_eq!(session.fetch_missing(&mut network, &directory).attempts, 0);
    assert_eq!(network.chunk_requests.len(), 4);
    let result = session.finish()?;
    assert_eq!(result.bytes, b"abcdefgh");
    assert_eq!(
        result.providers.into_iter().collect::<Vec<_>>(),
        vec![PeerId(2)]
    );
    Ok(())
}

#[test]
fn false_provider_advertisements_are_only_hints() -> Result<(), Error> {
    let mut network = network(1, 4096)?;
    let mut directory = MemoryDirectory::default();
    let publication = Publication::new(b"abcdefgh", 4, Limits::default())?;
    publish(&publication, 1, &mut network, &mut directory)?;
    for id in publication
        .manifest()
        .chunks()
        .iter()
        .copied()
        .chain([publication.id()])
    {
        directory.advertise(id, PeerId(0));
    }
    assert_eq!(
        download(
            publication.id(),
            DownloadOptions::default(),
            &mut network,
            &directory
        )?
        .bytes,
        b"abcdefgh"
    );
    Ok(())
}
