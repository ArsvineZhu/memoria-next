use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::fs::File;

use memoria_types::{MemoriaError, SourceBlobHash};

use crate::StoreLayout;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

type DirectorySync = fn(&Path) -> io::Result<()>;

#[derive(Clone, Debug)]
pub struct SourceCas {
    objects_dir: PathBuf,
    runtime_dir: PathBuf,
    directory_sync: DirectorySync,
}

impl SourceCas {
    #[must_use]
    pub fn new(layout: &StoreLayout) -> Self {
        Self {
            objects_dir: layout.objects_dir().to_path_buf(),
            runtime_dir: layout.runtime_dir().to_path_buf(),
            directory_sync: sync_directory_if_supported,
        }
    }

    #[cfg(test)]
    fn with_directory_sync(layout: &StoreLayout, directory_sync: DirectorySync) -> Self {
        Self {
            objects_dir: layout.objects_dir().to_path_buf(),
            runtime_dir: layout.runtime_dir().to_path_buf(),
            directory_sync,
        }
    }

    pub fn put(&self, source: &[u8]) -> Result<SourceBlobHash, MemoriaError> {
        let hash = SourceBlobHash::from_bytes(source);
        let object_path = self.object_path(hash);
        if object_is_file(&object_path)? {
            self.verify_existing_object(&object_path, hash)?;
            self.sync_objects_directory()?;
            return Ok(hash);
        }

        let staging_path = self.staging_path(hash);
        let mut staging_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)?;
        let _staging_cleanup = StagingCleanup::new(staging_path.clone());

        let write_result = staging_file
            .write_all(source)
            .and_then(|()| staging_file.sync_all());
        drop(staging_file);
        write_result?;

        if object_is_file(&object_path)? {
            self.verify_existing_object(&object_path, hash)?;
            self.sync_objects_directory()?;
            return Ok(hash);
        }

        match fs::rename(&staging_path, &object_path) {
            Ok(()) => {
                self.sync_objects_directory()?;
                Ok(hash)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if object_is_file(&object_path)? {
                    self.verify_existing_object(&object_path, hash)?;
                    self.sync_objects_directory()?;
                    Ok(hash)
                } else {
                    Err(error.into())
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn get(&self, hash: SourceBlobHash) -> Result<Vec<u8>, MemoriaError> {
        Ok(fs::read(self.object_path(hash))?)
    }

    pub fn contains(&self, hash: SourceBlobHash) -> Result<bool, MemoriaError> {
        Ok(object_is_file(&self.object_path(hash))?)
    }

    pub fn remove_if_exists(&self, hash: SourceBlobHash) -> Result<bool, MemoriaError> {
        let path = self.object_path(hash);
        match fs::remove_file(&path) {
            Ok(()) => {
                self.sync_objects_directory()?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    fn object_path(&self, hash: SourceBlobHash) -> PathBuf {
        self.objects_dir.join(hash.to_string())
    }

    fn verify_existing_object(
        &self,
        object_path: &Path,
        expected_hash: SourceBlobHash,
    ) -> Result<(), MemoriaError> {
        let bytes = fs::read(object_path)?;
        let actual_hash = SourceBlobHash::from_bytes(&bytes);
        if actual_hash != expected_hash {
            return Err(MemoriaError::Corruption {
                message: format!(
                    "source CAS object {} has hash {}, expected {}",
                    object_path.display(),
                    actual_hash,
                    expected_hash
                ),
            });
        }
        Ok(())
    }

    fn staging_path(&self, hash: SourceBlobHash) -> PathBuf {
        let sequence = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
        self.runtime_dir.join(format!(
            ".source-cas-{}-{}-{sequence}.tmp",
            hash,
            std::process::id()
        ))
    }

    fn sync_objects_directory(&self) -> io::Result<()> {
        (self.directory_sync)(&self.objects_dir)
    }
}

struct StagingCleanup {
    path: PathBuf,
}

impl StagingCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn object_is_file(path: &Path) -> std::io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn sync_directory_if_supported(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory_if_supported(path: &Path) -> std::io::Result<()> {
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Error, ErrorKind};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use memoria_types::MemoriaError;

    use super::{SourceCas, StoreLayout};

    static DIRECTORY_SYNC_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn recording_directory_sync(_: &std::path::Path) -> std::io::Result<()> {
        DIRECTORY_SYNC_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn failing_directory_sync(_: &std::path::Path) -> std::io::Result<()> {
        Err(Error::other("test directory sync failure"))
    }

    #[test]
    fn put_reuse_syncs_objects_directory_before_success() {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let cas = SourceCas::with_directory_sync(&layout, recording_directory_sync);
        DIRECTORY_SYNC_CALLS.store(0, Ordering::SeqCst);

        let hash = cas.put(b"reused source").unwrap();
        assert_eq!(DIRECTORY_SYNC_CALLS.load(Ordering::SeqCst), 1);

        assert_eq!(cas.put(b"reused source").unwrap(), hash);
        assert_eq!(DIRECTORY_SYNC_CALLS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn put_reuse_propagates_directory_sync_failure() {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let hash = memoria_types::SourceBlobHash::from_bytes(b"reused source");
        std::fs::write(
            layout.objects_dir().join(hash.to_string()),
            b"reused source",
        )
        .unwrap();
        let cas = SourceCas::with_directory_sync(&layout, failing_directory_sync);

        let error = cas.put(b"reused source").unwrap_err();
        assert!(matches!(
            error,
            MemoriaError::Io(error) if error.kind() == ErrorKind::Other
        ));
    }

    #[test]
    fn existing_corrupt_object_is_never_silently_reused() {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let source = b"expected source";
        let hash = memoria_types::SourceBlobHash::from_bytes(source);
        std::fs::write(layout.objects_dir().join(hash.to_string()), b"tampered").unwrap();
        let cas = SourceCas::new(&layout);

        assert!(matches!(
            cas.put(source),
            Err(MemoriaError::Corruption { .. })
        ));
    }

    #[test]
    fn existing_valid_object_is_reused_after_hash_verification() {
        let directory = tempfile::tempdir().unwrap();
        let layout = StoreLayout::create(directory.path()).unwrap();
        let source = b"already durable";
        let hash = memoria_types::SourceBlobHash::from_bytes(source);
        std::fs::write(layout.objects_dir().join(hash.to_string()), source).unwrap();
        let cas = SourceCas::new(&layout);

        assert_eq!(cas.put(source).unwrap(), hash);
    }
}
