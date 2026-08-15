use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;
use memoria_types::{MemoriaError, SourceBlobHash};

use crate::StoreLayout;

#[derive(Clone, Debug)]
pub struct SourceCas {
    objects_dir: PathBuf,
}

impl SourceCas {
    #[must_use]
    pub fn new(layout: &StoreLayout) -> Self {
        Self {
            objects_dir: layout.objects_dir().to_path_buf(),
        }
    }

    pub fn put(&self, source: &[u8]) -> Result<SourceBlobHash, MemoriaError> {
        let hash = SourceBlobHash::from_bytes(source);
        let object_path = self.object_path(hash);
        if object_is_file(&object_path)? {
            self.verify_existing_object(&object_path, hash)?;
            return Ok(hash);
        }

        let mut atomic_file = AtomicWriteFile::open(&object_path)?;
        atomic_file.write_all(source)?;
        atomic_file.commit()?;
        self.verify_existing_object(&object_path, hash)?;
        Ok(hash)
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
            Ok(()) => Ok(true),
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
}

fn object_is_file(path: &Path) -> std::io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
