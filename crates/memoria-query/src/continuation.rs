use std::time::{Duration, SystemTime};

use memoria_derived::{DerivedCatalog, ManifestLease};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::compile::CompiledQuery;
use crate::model::MemoryQuery;
use crate::snapshot::QuerySnapshot;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SessionError {
    #[error("read session or continuation has expired")]
    Expired,

    #[error("continuation query fingerprint does not match the session")]
    QueryFingerprintMismatch,

    #[error("continuation snapshot does not match the session")]
    SnapshotMismatch,

    #[error("the pinned Derived snapshot is unavailable: {message}")]
    SnapshotUnavailable { message: String },

    #[error("session TTL cannot be represented")]
    InvalidTtl,
}

pub struct ReadSession {
    snapshot: QuerySnapshot,
    query_fingerprint: [u8; 32],
    expires_at: SystemTime,
    _lease: Option<ManifestLease>,
}

impl ReadSession {
    pub fn open(
        catalog: &mut DerivedCatalog,
        compiled: &CompiledQuery,
        ttl: Duration,
    ) -> Result<Self, SessionError> {
        let lease = catalog
            .acquire_lease(compiled.snapshot.derived_manifest, ttl)
            .map_err(|error| SessionError::SnapshotUnavailable {
                message: error.to_string(),
            })?;
        Self::from_compiled(compiled, ttl, Some(lease))
    }

    pub fn new(compiled: &CompiledQuery, ttl: Duration) -> Result<Self, SessionError> {
        Self::from_compiled(compiled, ttl, None)
    }

    fn from_compiled(
        compiled: &CompiledQuery,
        ttl: Duration,
        lease: Option<ManifestLease>,
    ) -> Result<Self, SessionError> {
        let expires_at = SystemTime::now()
            .checked_add(ttl)
            .ok_or(SessionError::InvalidTtl)?;
        Ok(Self {
            snapshot: compiled.snapshot.clone(),
            query_fingerprint: fingerprint(&compiled.query),
            expires_at,
            _lease: lease,
        })
    }

    #[must_use]
    pub const fn snapshot(&self) -> &QuerySnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expires_at
    }

    pub fn continuation(&self, cursor: usize) -> Result<Continuation, SessionError> {
        if self.is_expired() {
            return Err(SessionError::Expired);
        }
        Ok(Continuation {
            snapshot: self.snapshot.clone(),
            query_fingerprint: self.query_fingerprint,
            cursor,
            expires_at: self.expires_at,
        })
    }

    pub fn validate_continuation(
        &self,
        continuation: &Continuation,
        query: &MemoryQuery,
    ) -> Result<usize, SessionError> {
        if self.is_expired() || continuation.is_expired() {
            return Err(SessionError::Expired);
        }
        if continuation.snapshot != self.snapshot {
            return Err(SessionError::SnapshotMismatch);
        }
        if continuation.query_fingerprint != fingerprint(query)
            || continuation.query_fingerprint != self.query_fingerprint
        {
            return Err(SessionError::QueryFingerprintMismatch);
        }
        Ok(continuation.cursor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Continuation {
    snapshot: QuerySnapshot,
    query_fingerprint: [u8; 32],
    pub cursor: usize,
    expires_at: SystemTime,
}

impl Continuation {
    #[must_use]
    pub fn snapshot(&self) -> &QuerySnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn is_expired(&self) -> bool {
        SystemTime::now() >= self.expires_at
    }
}

fn fingerprint(query: &MemoryQuery) -> [u8; 32] {
    Sha256::digest(format!("{query:?}").as_bytes()).into()
}
