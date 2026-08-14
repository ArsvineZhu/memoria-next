use std::path::Path;

use memoria_types::StoreId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupManifest {
    pub store_id: StoreId,
    pub includes_adaptive: bool,
    pub file_count: usize,
}

/// Inspect the identity-bearing portion of a staged backup before activation.
pub fn inspect_backup(
    backup_dir: impl AsRef<Path>,
    includes_adaptive: bool,
    file_count: usize,
) -> Result<BackupManifest, std::io::Error> {
    let backup_dir = backup_dir.as_ref();
    let store_id = std::fs::read_to_string(backup_dir.join("STORE"))?
        .trim()
        .parse::<StoreId>()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string()))?;
    if !backup_dir
        .join("authority")
        .join("authority.sqlite")
        .is_file()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "backup is missing authority/authority.sqlite",
        ));
    }
    Ok(BackupManifest {
        store_id,
        includes_adaptive,
        file_count,
    })
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
