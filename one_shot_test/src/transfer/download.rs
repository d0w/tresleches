use std::collections::BTreeSet;

use crate::{
    Error, PeerId,
    discovery::ProviderDirectory,
    protocol::{Assembler, ContentId, Limits},
    transport::ContentTransport,
};

/// Local validation and per-round request bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOptions {
    /// Limits applied independently of a publisher's or provider's limits.
    pub limits: Limits,
    /// Maximum distinct providers attempted per object per explicit fetch round.
    pub max_provider_attempts: usize,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            max_provider_attempts: 8,
        }
    }
}

/// Snapshot of retained progress after a bounded fetch round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    /// Verified chunk positions retained across rounds.
    pub received: usize,
    /// Expected chunk positions.
    pub total: usize,
    /// Chunk requests made in this round, including unsuccessful ones.
    pub attempts: usize,
}

/// Verified data and the peers that supplied its chunks.
#[derive(Debug, PartialEq, Eq)]
pub struct DownloadReceipt {
    /// The requested root address.
    pub content_id: ContentId,
    /// Reconstructed application bytes.
    pub bytes: Vec<u8>,
    /// Peers that supplied verified chunks; excludes manifest-only providers.
    pub providers: BTreeSet<PeerId>,
}

/// Receiver-owned transfer state that survives supplying-peer disconnects.
/// State is in memory only; the caller decides whether and when to retry.
#[derive(Debug)]
pub struct DownloadSession {
    content_id: ContentId,
    assembly: Assembler,
    providers: BTreeSet<PeerId>,
    provider_offsets: Vec<usize>,
    options: DownloadOptions,
}

impl DownloadSession {
    /// Looks up and verifies a manifest against the caller's trusted root ID.
    /// Stale, unavailable, invalid, or oversized candidates are skipped.
    pub fn open(
        content_id: ContentId,
        options: DownloadOptions,
        transport: &mut impl ContentTransport,
        directory: &impl ProviderDirectory,
    ) -> Result<Self, Error> {
        if options.max_provider_attempts == 0 {
            return Err(Error::InvalidConfig(
                "provider attempt limit must be positive",
            ));
        }
        for peer in candidates(directory, content_id, options.max_provider_attempts, 0) {
            let Ok(manifest) = transport.get_manifest(peer, content_id) else {
                continue;
            };
            if manifest.validate(options.limits).is_err() || manifest.id() != content_id {
                continue;
            }
            let provider_offsets = vec![0; manifest.chunks().len()];
            return Ok(Self {
                content_id,
                assembly: Assembler::new(manifest, options.limits)?,
                providers: BTreeSet::new(),
                provider_offsets,
                options,
            });
        }
        Err(Error::ManifestUnavailable(content_id))
    }

    /// Attempts each missing position using a bounded number of providers.
    ///
    /// A successful request is the acknowledgment for that chunk. Invalid data,
    /// missing objects, and transport failures trigger another candidate. If all
    /// fail, the chunk remains missing. Already verified positions are not fetched
    /// again. Candidate windows rotate between rounds so a fixed set of stale
    /// records cannot permanently hide providers beyond the per-round limit.
    pub fn fetch_missing(
        &mut self,
        transport: &mut impl ContentTransport,
        directory: &impl ProviderDirectory,
    ) -> DownloadProgress {
        let missing: Vec<_> = self.assembly.missing_indices().collect();
        let mut attempts = 0;
        for index in missing {
            let id = self.assembly.manifest().chunks()[index];
            for peer in candidates(
                directory,
                id,
                self.options.max_provider_attempts,
                self.provider_offsets[index],
            ) {
                attempts += 1;
                self.provider_offsets[index] = self.provider_offsets[index].wrapping_add(1);
                let Ok(bytes) = transport.get_chunk(peer, id) else {
                    continue;
                };
                if self.assembly.accept(index, &bytes).is_ok() {
                    self.providers.insert(peer);
                    break;
                }
            }
        }
        DownloadProgress {
            received: self.assembly.received(),
            total: self.assembly.manifest().chunks().len(),
            attempts,
        }
    }

    /// Whether every expected chunk has been independently verified.
    pub fn is_complete(&self) -> bool {
        self.assembly.is_complete()
    }

    /// Positions still needed; inspecting progress does not consume the session.
    pub fn missing_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.assembly.missing_indices()
    }

    /// Delivers complete data, or fails without returning partial data.
    /// This consumes the session; check completion first if retries are desired.
    pub fn finish(self) -> Result<DownloadReceipt, Error> {
        Ok(DownloadReceipt {
            content_id: self.content_id,
            bytes: self.assembly.finish()?,
            providers: self.providers,
        })
    }
}

/// Convenience for one manifest lookup and one bounded round of chunk retrieval.
/// Use `DownloadSession` directly to retain partial progress after failures.
pub fn download(
    id: ContentId,
    options: DownloadOptions,
    transport: &mut impl ContentTransport,
    directory: &impl ProviderDirectory,
) -> Result<DownloadReceipt, Error> {
    let mut session = DownloadSession::open(id, options, transport, directory)?;
    session.fetch_missing(transport, directory);
    session.finish()
}

fn candidates(
    directory: &impl ProviderDirectory,
    id: ContentId,
    limit: usize,
    offset: usize,
) -> impl Iterator<Item = PeerId> {
    let peers: Vec<_> = directory
        .providers(id)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let count = peers.len();
    let start = if count == 0 { 0 } else { offset % count };
    peers.into_iter().cycle().skip(start).take(limit.min(count))
}
