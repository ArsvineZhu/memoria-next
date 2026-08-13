use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::fs::File;

use memoria_types::{MemoriaError, SourceBlobHash};

use crate::StoreLayout;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct SourceCas {
    objects_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl SourceCas {
    #[must_use]
    pub fn new(layout: &StoreLayout) -> Self {
        Self {
            objects_dir: layout.objects_dir().to_path_buf(),
            runtime_dir: layout.runtime_dir().to_path_buf(),
        }
    }

    pub fn put(&self, source: &[u8]) -> Result<SourceBlobHash, MemoriaError> {
        let hash = SourceBlobHash::from_bytes(source);
        let object_path = self.object_path(hash);
        if object_is_file(&object_path)? {
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
            return Ok(hash);
        }

        match fs::rename(&staging_path, &object_path) {
            Ok(()) => {
                sync_directory_if_supported(&self.objects_dir)?;
                Ok(hash)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if object_is_file(&object_path)? {
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

    fn object_path(&self, hash: SourceBlobHash) -> PathBuf {
        self.objects_dir.join(hash.to_string())
    }

    fn staging_path(&self, hash: SourceBlobHash) -> PathBuf {
        let sequence = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
        self.runtime_dir.join(format!(
            ".source-cas-{}-{}-{sequence}.tmp",
            hash,
            std::process::id()
        ))
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
