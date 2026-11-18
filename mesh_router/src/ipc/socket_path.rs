use std::{fs, io, os::unix::fs::MetadataExt, path::PathBuf};

/// Handles socket graceful handling
pub(super) struct SocketPathGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl SocketPathGuard {
    pub(super) fn new(path: &str) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;

        Ok(Self {
            path: PathBuf::from(path),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

impl Drop for SocketPathGuard {
    fn drop(&mut self) {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.dev() == self.device && metadata.ino() == self.inode => {
                if let Err(error) = fs::remove_file(&self.path) {
                    if error.kind() != io::ErrorKind::NotFound {
                        eprintln!("socket cleanup failed: {error}")
                    }
                }
            }

            Ok(_) => {
                eprintln!("socket pathname replaced; leaving untouched")
            }

            Err(error) if error.kind() == io::ErrorKind::NotFound => {}

            Err(error) => {
                eprintln!("socket inspection failed: {error}");
            }
        }
    }
}
