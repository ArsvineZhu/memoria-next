use std::collections::BTreeMap;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use memoria_adaptive::AdaptiveEventLog;
use memoria_authority::{AuthorityDb, StoreLayout};
use memoria_types::{AuthorityGeneration, MemoriaError, SourceBlobHash, StoreId};
use rusqlite::{Connection, backup::Backup};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const BACKUP_FORMAT: &str = "memoria-backup-v2";
const MANIFEST_NAME: &str = "backup-manifest.json";
const COMPLETE_NAME: &str = "COMPLETE";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupManifest {
    pub path: PathBuf,
    pub store_id: StoreId,
    pub authority_generation: AuthorityGeneration,
    pub includes_adaptive: bool,
    pub source_objects: Vec<SourceBlobHash>,
    pub file_count: usize,
    pub manifest_hash: String,
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("backup I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("backup SQLite error: {0}")]
    Sql(#[from] rusqlite::Error),

    #[error("backup Authority error: {0}")]
    Authority(#[from] MemoriaError),

    #[error("backup manifest serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid backup: {message}")]
    Invalid { message: String },

    #[error("backup target already exists: {path}")]
    AlreadyExists { path: PathBuf },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BackupManifestDocument {
    format: String,
    store_id: String,
    authority_generation: String,
    includes_adaptive: bool,
    source_objects: Vec<String>,
    files: BTreeMap<String, String>,
}

/// Create a consistent backup from the open Store's managed Authority and Adaptive planes.
pub fn create_backup(
    layout: &StoreLayout,
    authority: &AuthorityDb,
    adaptive: &AdaptiveEventLog,
    destination: Option<&Path>,
    includes_adaptive: bool,
) -> Result<BackupManifest, BackupError> {
    let destination = destination
        .map(Path::to_path_buf)
        .unwrap_or_else(default_backup_path);
    if destination.exists() {
        return Err(BackupError::AlreadyExists { path: destination });
    }
    fs::create_dir_all(destination.join("authority/objects"))?;
    if includes_adaptive {
        fs::create_dir_all(destination.join("adaptive"))?;
    }

    let generation = authority.current_generation()?;
    let source_objects = authority.source_blob_hashes_at(generation)?;
    copy_file_sync(
        &layout.store_dir().join("STORE"),
        &destination.join("STORE"),
    )?;
    online_backup(
        &layout.authority_database().to_path_buf(),
        &destination.join("authority/authority.sqlite"),
    )?;

    for hash in &source_objects {
        let source = layout.objects_dir().join(hash.to_string());
        let target = destination.join("authority/objects").join(hash.to_string());
        copy_verified_file(&source, &target, *hash)?;
    }

    if includes_adaptive {
        let adaptive_database = adaptive
            .database_path()
            .ok_or_else(|| BackupError::Invalid {
                message: "Adaptive log is not backed by a database".to_owned(),
            })?;
        online_backup(
            adaptive_database,
            &destination.join("adaptive/adaptive.sqlite"),
        )?;
    }

    let files = backup_file_hashes(&destination, includes_adaptive, &source_objects)?;
    let document = BackupManifestDocument {
        format: BACKUP_FORMAT.to_owned(),
        store_id: layout.store_id().to_string(),
        authority_generation: generation.to_string(),
        includes_adaptive,
        source_objects: source_objects.iter().map(ToString::to_string).collect(),
        files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&document)?;
    write_synced_file(
        &destination.join(format!("{MANIFEST_NAME}.tmp")),
        &manifest_bytes,
    )?;
    fs::rename(
        destination.join(format!("{MANIFEST_NAME}.tmp")),
        destination.join(MANIFEST_NAME),
    )?;
    write_synced_file(&destination.join(COMPLETE_NAME), b"complete\n")?;
    sync_directory(&destination)?;

    Ok(BackupManifest {
        path: destination,
        store_id: layout.store_id(),
        authority_generation: generation,
        includes_adaptive,
        source_objects,
        file_count: document.files.len(),
        manifest_hash: hex_digest(&manifest_bytes),
    })
}

/// Validate a staged backup and atomically activate it at `target`.
pub fn restore_backup(
    backup_dir: impl AsRef<Path>,
    target: impl AsRef<Path>,
) -> Result<BackupManifest, BackupError> {
    let backup_dir = backup_dir.as_ref();
    let target = target.as_ref();
    if target.exists() {
        return Err(BackupError::AlreadyExists {
            path: target.to_path_buf(),
        });
    }
    let complete = fs::read_to_string(backup_dir.join(COMPLETE_NAME))?;
    if complete.trim() != "complete" {
        return Err(invalid("backup COMPLETE marker is invalid"));
    }
    let manifest_bytes = fs::read(backup_dir.join(MANIFEST_NAME))?;
    let document: BackupManifestDocument = serde_json::from_slice(&manifest_bytes)?;
    let manifest = decode_manifest(&document)?;
    validate_backup_files(backup_dir, &document)?;
    validate_authority_database(&backup_dir.join("authority/authority.sqlite"))?;

    let parent = target
        .parent()
        .ok_or_else(|| invalid("restore target has no parent directory"))?;
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(
        ".{}-restore-{}-{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("store"),
        std::process::id(),
        unique_suffix()
    ));
    if stage.exists() {
        return Err(BackupError::AlreadyExists { path: stage });
    }
    fs::create_dir_all(stage.join("authority/objects"))?;
    fs::create_dir_all(stage.join("derived"))?;
    fs::create_dir_all(stage.join("cache"))?;
    fs::create_dir_all(stage.join("runtime"))?;
    if manifest.includes_adaptive {
        fs::create_dir_all(stage.join("adaptive"))?;
    }

    let copy_result = (|| {
        copy_file_sync(&backup_dir.join("STORE"), &stage.join("STORE"))?;
        copy_file_sync(
            &backup_dir.join("authority/authority.sqlite"),
            &stage.join("authority/authority.sqlite"),
        )?;
        for hash in &manifest.source_objects {
            let relative = PathBuf::from("authority/objects").join(hash.to_string());
            copy_file_sync(&backup_dir.join(&relative), &stage.join(&relative))?;
        }
        if manifest.includes_adaptive {
            copy_file_sync(
                &backup_dir.join("adaptive/adaptive.sqlite"),
                &stage.join("adaptive/adaptive.sqlite"),
            )?;
        }
        validate_authority_database(&stage.join("authority/authority.sqlite"))?;
        Ok::<(), BackupError>(())
    })();
    if let Err(error) = copy_result {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    if let Err(error) = fs::rename(&stage, target) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error.into());
    }
    let mut manifest = manifest;
    manifest.path = target.to_path_buf();
    Ok(manifest)
}

/// Inspect the identity-bearing portion of a staged backup before activation.
pub fn inspect_backup(
    backup_dir: impl AsRef<Path>,
    includes_adaptive: bool,
    file_count: usize,
) -> Result<BackupManifest, io::Error> {
    let backup_dir = backup_dir.as_ref();
    let store_id = fs::read_to_string(backup_dir.join("STORE"))?
        .trim()
        .parse::<StoreId>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if !backup_dir
        .join("authority")
        .join("authority.sqlite")
        .is_file()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "backup is missing authority/authority.sqlite",
        ));
    }
    Ok(BackupManifest {
        path: backup_dir.to_path_buf(),
        store_id,
        authority_generation: AuthorityGeneration::initial(),
        includes_adaptive,
        source_objects: Vec::new(),
        file_count,
        manifest_hash: String::new(),
    })
}

fn decode_manifest(document: &BackupManifestDocument) -> Result<BackupManifest, BackupError> {
    if document.format != BACKUP_FORMAT {
        return Err(invalid("unsupported backup format"));
    }
    let store_id = document
        .store_id
        .parse::<StoreId>()
        .map_err(|error| invalid(error.to_string()))?;
    let authority_generation = document
        .authority_generation
        .parse::<AuthorityGeneration>()
        .map_err(|error| invalid(error.to_string()))?;
    let source_objects = document
        .source_objects
        .iter()
        .map(|value| {
            value
                .parse::<SourceBlobHash>()
                .map_err(|error| invalid(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BackupManifest {
        path: PathBuf::new(),
        store_id,
        authority_generation,
        includes_adaptive: document.includes_adaptive,
        source_objects,
        file_count: document.files.len(),
        manifest_hash: String::new(),
    })
}

fn validate_backup_files(
    backup_dir: &Path,
    document: &BackupManifestDocument,
) -> Result<(), BackupError> {
    for (relative, expected_hash) in &document.files {
        let relative = safe_relative_path(relative)?;
        let path = backup_dir.join(&relative);
        let actual_hash = hash_file(&path)?;
        if actual_hash != *expected_hash {
            return Err(invalid(format!("backup file hash mismatch: {relative:?}")));
        }
    }
    Ok(())
}

fn backup_file_hashes(
    destination: &Path,
    includes_adaptive: bool,
    source_objects: &[SourceBlobHash],
) -> Result<BTreeMap<String, String>, BackupError> {
    let mut files = BTreeMap::new();
    files.insert("STORE".to_owned(), hash_file(&destination.join("STORE"))?);
    files.insert(
        "authority/authority.sqlite".to_owned(),
        hash_file(&destination.join("authority/authority.sqlite"))?,
    );
    for hash in source_objects {
        let relative = format!("authority/objects/{hash}");
        files.insert(
            relative,
            hash_file(&destination.join("authority/objects").join(hash.to_string()))?,
        );
    }
    if includes_adaptive {
        files.insert(
            "adaptive/adaptive.sqlite".to_owned(),
            hash_file(&destination.join("adaptive/adaptive.sqlite"))?,
        );
    }
    Ok(files)
}

fn online_backup(source: &Path, destination: &Path) -> Result<(), BackupError> {
    let source_connection = Connection::open(source)?;
    let mut destination_connection = Connection::open(destination)?;
    let backup = Backup::new(&source_connection, &mut destination_connection)?;
    backup.run_to_completion(16, Duration::from_millis(10), None)?;
    drop(backup);
    drop(destination_connection);
    Ok(())
}

fn copy_verified_file(
    source: &Path,
    destination: &Path,
    expected_hash: SourceBlobHash,
) -> Result<(), BackupError> {
    let bytes = fs::read(source).map_err(|error| io_at(source, error))?;
    if SourceBlobHash::from_bytes(&bytes) != expected_hash {
        return Err(invalid(format!(
            "source object hash mismatch: {}",
            source.display()
        )));
    }
    write_synced_file(destination, &bytes)?;
    let copied = fs::read(destination).map_err(|error| io_at(destination, error))?;
    if SourceBlobHash::from_bytes(&copied) != expected_hash {
        return Err(invalid(format!(
            "backup object hash mismatch: {}",
            destination.display()
        )));
    }
    Ok(())
}

fn copy_file_sync(source: &Path, destination: &Path) -> Result<(), BackupError> {
    let bytes = fs::read(source).map_err(|error| io_at(source, error))?;
    write_synced_file(destination, &bytes)
}

fn validate_authority_database(path: &Path) -> Result<(), BackupError> {
    let connection = Connection::open(path)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(invalid(format!(
            "authority integrity check failed: {integrity}"
        )));
    }
    Ok(())
}

fn write_synced_file(path: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| io_at(path, error))?;
    file.write_all(bytes).map_err(|error| io_at(path, error))?;
    file.sync_all().map_err(|error| io_at(path, error))?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), BackupError> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, BackupError> {
    let bytes = fs::read(path).map_err(|error| io_at(path, error))?;
    Ok(hex_digest(&bytes))
}

fn io_at(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn safe_relative_path(value: &str) -> Result<PathBuf, BackupError> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid(format!("unsafe backup path: {value}")));
    }
    Ok(path)
}

fn default_backup_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "memoria-next-backup-{}-{}",
        std::process::id(),
        unique_suffix()
    ))
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn invalid(message: impl Into<String>) -> BackupError {
    BackupError::Invalid {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::inspect_backup;
    use memoria_types::StoreId;
    use tempfile::tempdir;

    #[test]
    fn backup_inspection_preserves_store_identity() {
        let directory = tempdir().unwrap();
        let store_id = StoreId::from_bytes([7; 16]);
        fs::create_dir_all(directory.path().join("authority")).unwrap();
        fs::write(directory.path().join("STORE"), store_id.to_string()).unwrap();
        fs::write(
            directory.path().join("authority").join("authority.sqlite"),
            [],
        )
        .unwrap();

        let manifest = inspect_backup(directory.path(), true, 2).unwrap();
        assert_eq!(manifest.store_id, store_id);
        assert!(manifest.includes_adaptive);
        assert_eq!(manifest.file_count, 2);
    }
}
