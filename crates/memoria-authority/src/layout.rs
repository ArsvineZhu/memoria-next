use std::fs::{self, OpenOptions};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};

use memoria_types::{MemoriaError, StoreId};

const STORE_MARKER: &str = "STORE";
const WRITER_LOCK: &str = "writer.lock";
const AUTHORITY_DIR: &str = "authority";
const AUTHORITY_DATABASE: &str = "authority.sqlite";
const OBJECTS_DIR: &str = "objects";
const DERIVED_DIR: &str = "derived";
const ADAPTIVE_DIR: &str = "adaptive";
const CACHE_DIR: &str = "cache";
const RUNTIME_DIR: &str = "runtime";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreLayout {
    store_dir: PathBuf,
    store_id: StoreId,
    store_marker: PathBuf,
    authority_dir: PathBuf,
    authority_database: PathBuf,
    objects_dir: PathBuf,
    derived_dir: PathBuf,
    adaptive_dir: PathBuf,
    cache_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl StoreLayout {
    pub fn create(store_dir: impl AsRef<Path>) -> Result<Self, MemoriaError> {
        let mut layout = Self::paths(store_dir.as_ref());
        fs::create_dir_all(&layout.store_dir)?;
        fs::create_dir_all(&layout.authority_dir)?;
        fs::create_dir_all(&layout.objects_dir)?;
        fs::create_dir_all(&layout.derived_dir)?;
        fs::create_dir_all(&layout.adaptive_dir)?;
        fs::create_dir_all(&layout.cache_dir)?;
        fs::create_dir_all(&layout.runtime_dir)?;

        layout.store_id = if layout.store_marker.exists() {
            read_store_id(&layout.store_marker)?
        } else {
            let store_id = StoreId::try_new()?;
            let mut marker = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&layout.store_marker)?;
            marker.write_all(store_id.to_string().as_bytes())?;
            store_id
        };

        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&layout.authority_database)?;

        Ok(layout)
    }

    pub fn open(store_dir: impl AsRef<Path>) -> Result<Self, MemoriaError> {
        let mut layout = Self::paths(store_dir.as_ref());
        ensure_directory(&layout.store_dir)?;
        ensure_file(&layout.store_marker)?;
        ensure_directory(&layout.authority_dir)?;
        ensure_file(&layout.authority_database)?;
        ensure_directory(&layout.objects_dir)?;
        ensure_directory(&layout.derived_dir)?;
        ensure_directory(&layout.adaptive_dir)?;
        ensure_directory(&layout.cache_dir)?;
        ensure_directory(&layout.runtime_dir)?;

        layout.store_id = read_store_id(&layout.store_marker)?;
        Ok(layout)
    }

    #[must_use]
    pub fn store_dir(&self) -> &Path {
        &self.store_dir
    }

    #[must_use]
    pub fn store_id(&self) -> StoreId {
        self.store_id
    }

    #[must_use]
    pub fn authority_dir(&self) -> &Path {
        &self.authority_dir
    }

    #[must_use]
    pub fn authority_database(&self) -> &Path {
        &self.authority_database
    }

    #[must_use]
    pub fn objects_dir(&self) -> &Path {
        &self.objects_dir
    }

    #[must_use]
    pub fn derived_dir(&self) -> &Path {
        &self.derived_dir
    }

    #[must_use]
    pub fn adaptive_dir(&self) -> &Path {
        &self.adaptive_dir
    }

    #[must_use]
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    #[must_use]
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    pub(crate) fn writer_lock_path(&self) -> PathBuf {
        self.runtime_dir.join(WRITER_LOCK)
    }

    fn paths(store_dir: &Path) -> Self {
        let store_dir = store_dir.to_path_buf();
        let store_marker = store_dir.join(STORE_MARKER);
        let authority_dir = store_dir.join(AUTHORITY_DIR);
        let authority_database = authority_dir.join(AUTHORITY_DATABASE);
        let objects_dir = authority_dir.join(OBJECTS_DIR);

        Self {
            store_dir: store_dir.clone(),
            store_id: StoreId::from_bytes([0; 16]),
            store_marker,
            authority_dir,
            authority_database,
            objects_dir,
            derived_dir: store_dir.join(DERIVED_DIR),
            adaptive_dir: store_dir.join(ADAPTIVE_DIR),
            cache_dir: store_dir.join(CACHE_DIR),
            runtime_dir: store_dir.join(RUNTIME_DIR),
        }
    }
}

fn read_store_id(path: &Path) -> Result<StoreId, MemoriaError> {
    fs::read_to_string(path)?.trim().parse()
}

fn ensure_directory(path: &Path) -> Result<(), MemoriaError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("expected directory: {}", path.display()),
        )
        .into())
    }
}

fn ensure_file(path: &Path) -> Result<(), MemoriaError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            format!("expected file: {}", path.display()),
        )
        .into())
    }
}
