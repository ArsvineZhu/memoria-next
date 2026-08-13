use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use memoria_types::MemoriaError;

use crate::StoreLayout;

#[derive(Debug)]
pub struct StoreWriterLock {
    _file: File,
    path: PathBuf,
}

impl StoreWriterLock {
    pub fn acquire(store_dir: impl AsRef<Path>) -> Result<Self, MemoriaError> {
        let layout = StoreLayout::open(store_dir)?;
        let path = layout.writer_lock_path();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| map_lock_error(&path, error))?;

        if let Err(error) = file.try_lock_exclusive() {
            return Err(map_lock_error(&path, error));
        }

        Ok(Self { _file: file, path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn map_lock_error(path: &Path, error: std::io::Error) -> MemoriaError {
    let contended = fs2::lock_contended_error();
    let is_contended = match (error.raw_os_error(), contended.raw_os_error()) {
        (Some(actual), Some(expected)) => actual == expected,
        _ => error.kind() == contended.kind(),
    };

    if is_contended {
        MemoriaError::StoreLocked {
            path: path.to_path_buf(),
        }
    } else {
        MemoriaError::Io(error)
    }
}
