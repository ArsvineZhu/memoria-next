use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::ManifestId;

pub struct ManifestLease {
    id: i64,
    manifest_id: ManifestId,
    database_path: PathBuf,
    expires_at: i64,
}

impl ManifestLease {
    pub(crate) fn new(
        id: i64,
        manifest_id: ManifestId,
        database_path: PathBuf,
        expires_at: i64,
    ) -> Self {
        Self {
            id,
            manifest_id,
            database_path,
            expires_at,
        }
    }

    #[must_use]
    pub const fn manifest_id(&self) -> ManifestId {
        self.manifest_id
    }

    #[must_use]
    pub const fn expires_at_unix_seconds(&self) -> i64 {
        self.expires_at
    }
}

impl Drop for ManifestLease {
    fn drop(&mut self) {
        let Ok(connection) = Connection::open(&self.database_path) else {
            return;
        };
        let _ = connection.execute("DELETE FROM leases WHERE id = ?1", [self.id]);
    }
}

pub(crate) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(i64::MAX)
}
