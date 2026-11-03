use super::{Limits, Manifest};
use crate::Error;

/// Bounded, out-of-order assembly independent of any supplying connection.
#[derive(Debug)]
pub struct Assembler {
    manifest: Manifest,
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
}

impl Assembler {
    /// Validates receiver limits before allocating chunk slots.
    pub fn new(manifest: Manifest, limits: Limits) -> Result<Self, Error> {
        manifest.validate(limits)?;
        let chunks = vec![None; manifest.chunks().len()];
        Ok(Self {
            manifest,
            chunks,
            received: 0,
        })
    }

    /// Accepts a verified chunk. Returns `false` for an already received chunk.
    /// Invalid input leaves assembly state unchanged.
    pub fn accept(&mut self, index: usize, bytes: &[u8]) -> Result<bool, Error> {
        self.manifest.verify_chunk(index, bytes)?;
        if self.chunks[index].is_some() {
            return Ok(false);
        }
        self.chunks[index] = Some(bytes.to_vec());
        self.received += 1;
        Ok(true)
    }

    /// The validated manifest for this transfer.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Number of independently verified chunk positions.
    pub fn received(&self) -> usize {
        self.received
    }

    /// Whether every expected chunk position has been received.
    pub fn is_complete(&self) -> bool {
        self.received == self.chunks.len()
    }

    /// Missing positions in reconstruction order.
    pub fn missing_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| chunk.is_none().then_some(index))
    }

    /// Reconstructs complete data, consuming the buffered chunks.
    /// Peak logical payload memory may reach twice the content size while copying.
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        if !self.is_complete() {
            return Err(Error::IncompleteTransfer {
                missing: self.chunks.len() - self.received,
            });
        }
        let mut bytes = Vec::with_capacity(self.manifest.total_len());
        for chunk in self.chunks.into_iter().flatten() {
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Publication;

    #[test]
    fn accepts_reordering_and_duplicates_but_not_corruption() -> Result<(), Error> {
        let publication = Publication::new(b"abcdefghij", 4, Limits::default())?;
        let mut assembly = Assembler::new(publication.manifest().clone(), Limits::default())?;
        assert!(assembly.accept(2, b"ij")?);
        assert!(!assembly.accept(2, b"ij")?);
        assert!(assembly.accept(0, b"xxxx").is_err());
        assert!(assembly.accept(0, b"abc").is_err());
        assert!(assembly.accept(100, b"").is_err());
        assert_eq!(assembly.received(), 1);
        assert_eq!(assembly.missing_indices().collect::<Vec<_>>(), vec![0, 1]);
        assembly.accept(1, b"efgh")?;
        assembly.accept(0, b"abcd")?;
        assert_eq!(assembly.finish()?, b"abcdefghij");
        Ok(())
    }

    #[test]
    fn empty_content_is_complete() -> Result<(), Error> {
        let publication = Publication::new(b"", 4, Limits::default())?;
        let assembly = Assembler::new(publication.manifest().clone(), Limits::default())?;
        assert!(assembly.is_complete());
        assert_eq!(assembly.finish()?, b"");
        Ok(())
    }

    #[test]
    fn incomplete_content_cannot_be_delivered() -> Result<(), Error> {
        let publication = Publication::new(b"abc", 1, Limits::default())?;
        let assembly = Assembler::new(publication.manifest().clone(), Limits::default())?;
        assert_eq!(
            assembly.finish(),
            Err(Error::IncompleteTransfer { missing: 3 })
        );
        Ok(())
    }
}
