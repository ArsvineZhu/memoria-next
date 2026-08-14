use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::fs::File;

use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::{
    DerivedError, EmbeddingNormalization, EmbeddingSignature, EmbeddingVector, ProjectionInputHash,
};

const VECTOR_PAYLOAD_MAGIC: &[u8; 8] = b"MEMVEC01";
pub const VECTOR_PAYLOAD_FORMAT_VERSION: u16 = 1;
const VECTOR_PAYLOAD_HEADER_LENGTH: usize = 8 + 2 + 4 + 1 + 32 + 32;
static NEXT_VECTOR_PAYLOAD_STAGING_ID: AtomicU64 = AtomicU64::new(0);
const ANN_SEGMENT_MAGIC: &[u8; 8] = b"MEMANN01";
const ANN_SEGMENT_FORMAT_VERSION: u16 = 1;
static NEXT_ANN_SEGMENT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

pub const ANN_DELTA_COMPACTION_THRESHOLD: usize = 8;
pub const ANN_TOMBSTONE_RATIO_PERCENT: u64 = 20;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VectorPayloadHash([u8; 32]);

impl VectorPayloadHash {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    #[must_use]
    pub fn as_hex(&self) -> String {
        hex_lower(&self.0)
    }
}

impl fmt::Display for VectorPayloadHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex_lower(&self.0))
    }
}

/// Canonical immutable vector payload stored below `derived/objects/vector`.
///
/// Membership identity deliberately does not live in this object. The same
/// payload can therefore be referenced by multiple revisions or Spaces.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorPayloadV1 {
    dimension: u32,
    normalization: EmbeddingNormalization,
    producer_signature_hash: [u8; 32],
    projection_input_hash: ProjectionInputHash,
    values: Vec<f32>,
}

impl VectorPayloadV1 {
    pub fn new(
        values: Vec<f32>,
        normalization: EmbeddingNormalization,
        producer_signature: impl AsRef<[u8]>,
        projection_input_hash: ProjectionInputHash,
    ) -> Result<Self, DerivedError> {
        let dimension =
            u32::try_from(values.len()).map_err(|_| DerivedError::InvalidProjectionValue {
                value: "vector payload dimension is out of range".to_owned(),
            })?;
        if dimension == 0 {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector payload dimension must be positive".to_owned(),
            });
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector payload values must be finite".to_owned(),
            });
        }
        Ok(Self {
            dimension,
            normalization,
            producer_signature_hash: producer_signature_hash(producer_signature.as_ref()),
            projection_input_hash,
            values,
        })
    }

    #[must_use]
    pub const fn dimension(&self) -> u32 {
        self.dimension
    }

    #[must_use]
    pub const fn normalization(&self) -> EmbeddingNormalization {
        self.normalization
    }

    #[must_use]
    pub const fn producer_signature_hash(&self) -> [u8; 32] {
        self.producer_signature_hash
    }

    #[must_use]
    pub fn projection_input_hash(&self) -> &ProjectionInputHash {
        &self.projection_input_hash
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }

    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(VECTOR_PAYLOAD_HEADER_LENGTH + self.values.len() * 4);
        bytes.extend_from_slice(VECTOR_PAYLOAD_MAGIC);
        bytes.extend_from_slice(&VECTOR_PAYLOAD_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.dimension.to_le_bytes());
        bytes.push(normalization_byte(self.normalization));
        bytes.extend_from_slice(&self.producer_signature_hash);
        bytes.extend_from_slice(self.projection_input_hash.as_bytes());
        for value in &self.values {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    }

    #[must_use]
    pub fn hash(&self) -> VectorPayloadHash {
        VectorPayloadHash::from_bytes(Sha256::digest(self.canonical_bytes()).into())
    }

    #[must_use]
    pub fn payload_hash(&self) -> VectorPayloadHash {
        self.hash()
    }

    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.canonical_bytes()
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DerivedError> {
        if bytes.len() < VECTOR_PAYLOAD_HEADER_LENGTH {
            return Err(invalid_payload("vector payload is truncated"));
        }
        if &bytes[..VECTOR_PAYLOAD_MAGIC.len()] != VECTOR_PAYLOAD_MAGIC {
            return Err(invalid_payload(
                "vector payload magic does not match MEMVEC01",
            ));
        }
        let mut cursor = VECTOR_PAYLOAD_MAGIC.len();
        let version = read_u16(bytes, &mut cursor)?;
        if version != VECTOR_PAYLOAD_FORMAT_VERSION {
            return Err(invalid_payload("unsupported vector payload format version"));
        }
        let dimension = read_u32(bytes, &mut cursor)?;
        if dimension == 0 {
            return Err(invalid_payload("vector payload dimension must be positive"));
        }
        let normalization = normalization_from_byte(read_byte(bytes, &mut cursor)?)?;
        let producer_signature_hash = read_fixed::<32>(bytes, &mut cursor)?;
        let projection_input_hash =
            ProjectionInputHash::from_bytes(read_fixed::<32>(bytes, &mut cursor)?);
        let value_bytes = usize::try_from(dimension)
            .ok()
            .and_then(|dimension| dimension.checked_mul(4))
            .ok_or_else(|| invalid_payload("vector payload dimension is too large"))?;
        let expected_length = cursor
            .checked_add(value_bytes)
            .ok_or_else(|| invalid_payload("vector payload length overflow"))?;
        if bytes.len() != expected_length {
            return Err(invalid_payload(
                "vector payload length does not match dimension",
            ));
        }
        let mut values = Vec::with_capacity(
            usize::try_from(dimension)
                .map_err(|_| invalid_payload("vector payload dimension is out of range"))?,
        );
        while cursor < bytes.len() {
            let bits = read_u32(bytes, &mut cursor)?;
            let value = f32::from_bits(bits);
            if !value.is_finite() {
                return Err(invalid_payload("vector payload values must be finite"));
            }
            values.push(value);
        }
        Ok(Self {
            dimension,
            normalization,
            producer_signature_hash,
            projection_input_hash,
            values,
        })
    }

    pub fn put(&self, derived_dir: impl AsRef<Path>) -> Result<VectorPayloadHash, DerivedError> {
        let hash = self.hash();
        let path = vector_payload_path(derived_dir.as_ref(), hash);
        if path.is_file() {
            verify_vector_payload_file(&path, hash)?;
            return Ok(hash);
        }
        if path.exists() {
            return Err(invalid_payload(
                "vector payload target is not a regular file",
            ));
        }

        let parent = path
            .parent()
            .ok_or_else(|| invalid_payload("vector payload path has no parent"))?;
        fs::create_dir_all(parent)?;
        let staging_path = parent.join(format!(
            ".{}.{}.tmp",
            hash,
            NEXT_VECTOR_PAYLOAD_STAGING_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut staging = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)?;
        let bytes = self.canonical_bytes();
        let write_result = staging.write_all(&bytes).and_then(|()| staging.sync_all());
        drop(staging);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&staging_path);
            return Err(error.into());
        }

        if path.is_file() {
            let _ = fs::remove_file(&staging_path);
            verify_vector_payload_file(&path, hash)?;
            return Ok(hash);
        }
        match fs::rename(&staging_path, &path) {
            Ok(()) => {
                sync_directory(parent)?;
                Ok(hash)
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging_path);
                verify_vector_payload_file(&path, hash)?;
                Ok(hash)
            }
            Err(error) => {
                let _ = fs::remove_file(&staging_path);
                Err(error.into())
            }
        }
    }

    pub fn get(
        derived_dir: impl AsRef<Path>,
        hash: VectorPayloadHash,
    ) -> Result<Self, DerivedError> {
        let path = vector_payload_path(derived_dir.as_ref(), hash);
        let bytes = fs::read(&path)?;
        let payload = Self::from_canonical_bytes(&bytes)?;
        if payload.hash() != hash {
            return Err(invalid_payload(
                "vector payload checksum does not match its path",
            ));
        }
        Ok(payload)
    }

    pub fn verify(
        derived_dir: impl AsRef<Path>,
        hash: VectorPayloadHash,
    ) -> Result<(), DerivedError> {
        Self::get(derived_dir, hash).map(|_| ())
    }
}

fn invalid_payload(value: impl Into<String>) -> DerivedError {
    DerivedError::InvalidProjectionValue {
        value: format!("vector payload: {}", value.into()),
    }
}

fn producer_signature_hash(signature: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"memoria-vector-producer-signature-v1\0");
    hasher.update(signature);
    hasher.finalize().into()
}

fn normalization_byte(normalization: EmbeddingNormalization) -> u8 {
    match normalization {
        EmbeddingNormalization::None => 0,
        EmbeddingNormalization::L2 => 1,
    }
}

fn normalization_from_byte(value: u8) -> Result<EmbeddingNormalization, DerivedError> {
    match value {
        0 => Ok(EmbeddingNormalization::None),
        1 => Ok(EmbeddingNormalization::L2),
        _ => Err(invalid_payload("unknown vector normalization")),
    }
}

fn read_byte(bytes: &[u8], cursor: &mut usize) -> Result<u8, DerivedError> {
    let value = *bytes
        .get(*cursor)
        .ok_or_else(|| invalid_payload("vector payload is truncated"))?;
    *cursor += 1;
    Ok(value)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, DerivedError> {
    Ok(u16::from_le_bytes(read_fixed(bytes, cursor)?))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, DerivedError> {
    Ok(u32::from_le_bytes(read_fixed(bytes, cursor)?))
}

fn read_fixed<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], DerivedError> {
    let end = cursor
        .checked_add(N)
        .ok_or_else(|| invalid_payload("vector payload length overflow"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| invalid_payload("vector payload is truncated"))?;
    let mut result = [0; N];
    result.copy_from_slice(value);
    *cursor = end;
    Ok(result)
}

fn vector_payload_path(derived_dir: &Path, hash: VectorPayloadHash) -> PathBuf {
    let hex = hash.as_hex();
    derived_dir
        .join("objects")
        .join("vector")
        .join(&hex[..2])
        .join(format!("{hex}.vec"))
}

fn verify_vector_payload_file(
    path: &Path,
    expected: VectorPayloadHash,
) -> Result<(), DerivedError> {
    let bytes = fs::read(path)?;
    let payload = VectorPayloadV1::from_canonical_bytes(&bytes)?;
    if payload.hash() != expected {
        return Err(invalid_payload("existing vector payload checksum mismatch"));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), DerivedError> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnSegmentEntry {
    target_key: String,
    membership: VectorMembership,
    values: Vec<f32>,
}

impl AnnSegmentEntry {
    pub fn new(
        target_key: impl Into<String>,
        membership: VectorMembership,
        values: Vec<f32>,
    ) -> Result<Self, DerivedError> {
        if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
            return Err(DerivedError::InvalidProjectionValue {
                value: "ANN segment vector values must be finite and non-empty".to_owned(),
            });
        }
        let target_key = target_key.into();
        if target_key.is_empty() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "ANN segment target key must not be empty".to_owned(),
            });
        }
        Ok(Self {
            target_key,
            membership,
            values,
        })
    }

    #[must_use]
    pub fn target_key(&self) -> &str {
        &self.target_key
    }

    #[must_use]
    pub const fn membership(&self) -> VectorMembership {
        self.membership
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// Immutable ANN delta segment with a canonical on-disk representation.
///
/// USearch remains the in-process query adapter. The serialized segment keeps
/// enough metadata to rebuild that adapter without consulting Authority.
pub struct AnnSegmentV1 {
    producer_signature: String,
    dimension: u32,
    entries: Vec<AnnSegmentEntry>,
    index: VectorIndex,
    synthetic_memberships: BTreeMap<VectorMembership, VectorMembership>,
}

impl AnnSegmentV1 {
    pub fn build(
        producer_signature: impl Into<String>,
        entries: Vec<AnnSegmentEntry>,
    ) -> Result<Self, DerivedError> {
        let producer_signature = producer_signature.into();
        if producer_signature.trim().is_empty() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "ANN segment producer signature must not be empty".to_owned(),
            });
        }
        let dimension = entries.first().map_or(0, |entry| entry.values.len());
        if dimension == 0 {
            return Err(DerivedError::InvalidProjectionValue {
                value: "ANN segment must contain at least one vector".to_owned(),
            });
        }
        let dimension_u32 =
            u32::try_from(dimension).map_err(|_| DerivedError::InvalidProjectionValue {
                value: "ANN segment dimension is out of range".to_owned(),
            })?;
        let mut by_target = BTreeMap::new();
        for entry in entries {
            if entry.values.len() != dimension {
                return Err(DerivedError::InvalidProjectionValue {
                    value: "ANN segment vectors have inconsistent dimensions".to_owned(),
                });
            }
            by_target.insert(entry.target_key.clone(), entry);
        }
        let entries = by_target.into_values().collect::<Vec<_>>();
        let signature = EmbeddingSignature::new(
            "ann-segment",
            producer_signature.clone(),
            "semantic",
            dimension,
            EmbeddingNormalization::None,
            1,
        )?;
        let mut index = VectorIndex::new(signature.clone())?;
        let mut synthetic_memberships = BTreeMap::new();
        for entry in &entries {
            let vector = EmbeddingVector::new(signature.clone(), entry.values.clone())?;
            let artifact = VectorArtifact::from_embedding(vector);
            let synthetic = VectorMembership::new(
                entry.membership.space_id(),
                entry.membership.memory_id(),
                entry.membership.revision_id(),
                entry.membership.authority_generation(),
                &artifact,
            );
            index.insert(artifact, synthetic)?;
            synthetic_memberships.insert(synthetic, entry.membership);
        }
        Ok(Self {
            producer_signature,
            dimension: dimension_u32,
            entries,
            index,
            synthetic_memberships,
        })
    }

    #[must_use]
    pub const fn dimension(&self) -> u32 {
        self.dimension
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }

    pub fn entries(&self) -> impl Iterator<Item = &AnnSegmentEntry> {
        self.entries.iter()
    }

    #[must_use]
    pub fn vector_count(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(ANN_SEGMENT_MAGIC);
        bytes.extend_from_slice(&ANN_SEGMENT_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.dimension.to_le_bytes());
        put_string_u32(&mut bytes, self.producer_signature.as_bytes());
        bytes.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for entry in &self.entries {
            put_string_u32(&mut bytes, entry.target_key.as_bytes());
            bytes.extend_from_slice(entry.membership.space_id().as_bytes());
            bytes.extend_from_slice(entry.membership.memory_id().as_bytes());
            bytes.extend_from_slice(entry.membership.revision_id().as_bytes());
            bytes.extend_from_slice(
                &entry
                    .membership
                    .authority_generation()
                    .value()
                    .to_le_bytes(),
            );
            let payload_hash = entry.membership.payload_hash();
            bytes.extend_from_slice(&payload_hash);
            for value in &entry.values {
                bytes.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        bytes
    }

    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, DerivedError> {
        if bytes.len() < 8 + 2 + 4 + 4 + 4 {
            return Err(invalid_payload("ANN segment is truncated"));
        }
        if &bytes[..8] != ANN_SEGMENT_MAGIC {
            return Err(invalid_payload("ANN segment magic does not match MEMANN01"));
        }
        let mut cursor = 8;
        let version = read_u16(bytes, &mut cursor)?;
        if version != ANN_SEGMENT_FORMAT_VERSION {
            return Err(invalid_payload("unsupported ANN segment format version"));
        }
        let dimension = read_u32(bytes, &mut cursor)?;
        if dimension == 0 {
            return Err(invalid_payload("ANN segment dimension must be positive"));
        }
        let producer_signature = read_string_u32(bytes, &mut cursor)?;
        let count = read_u32(bytes, &mut cursor)?;
        let mut entries = Vec::with_capacity(
            usize::try_from(count)
                .map_err(|_| invalid_payload("ANN segment vector count is out of range"))?,
        );
        for _ in 0..count {
            let target_key = read_string_u32(bytes, &mut cursor)?;
            let space_id = SpaceId::from_bytes(read_fixed(bytes, &mut cursor)?);
            let memory_id = MemoryId::from_bytes(read_fixed(bytes, &mut cursor)?);
            let revision_id = RevisionId::from_bytes(read_fixed(bytes, &mut cursor)?);
            let generation = AuthorityGeneration::new(read_u64(bytes, &mut cursor)?);
            let payload_hash = read_fixed::<32>(bytes, &mut cursor)?;
            let value_count = usize::try_from(dimension)
                .map_err(|_| invalid_payload("ANN segment dimension is out of range"))?;
            let mut values = Vec::with_capacity(value_count);
            for _ in 0..value_count {
                let value = f32::from_bits(read_u32(bytes, &mut cursor)?);
                if !value.is_finite() {
                    return Err(invalid_payload("ANN segment contains a non-finite value"));
                }
                values.push(value);
            }
            let membership = VectorMembership::from_parts(
                space_id,
                memory_id,
                revision_id,
                generation,
                payload_hash,
            );
            entries.push(AnnSegmentEntry::new(target_key, membership, values)?);
        }
        let encoded_dimension = entries.first().map_or(0, |entry| entry.values.len());
        let encoded_dimension = u32::try_from(encoded_dimension)
            .map_err(|_| invalid_payload("ANN segment dimension is out of range"))?;
        if cursor != bytes.len() || encoded_dimension != dimension {
            return Err(invalid_payload("ANN segment has an invalid encoded length"));
        }
        Self::build(producer_signature, entries)
    }

    pub fn put(&self, derived_dir: impl AsRef<Path>) -> Result<[u8; 32], DerivedError> {
        let hash = self.hash();
        let path = ann_segment_path(derived_dir.as_ref(), hash);
        if path.is_file() {
            verify_ann_segment_file(&path, hash)?;
            return Ok(hash);
        }
        if path.exists() {
            return Err(invalid_payload("ANN segment target is not a regular file"));
        }
        let parent = path
            .parent()
            .ok_or_else(|| invalid_payload("ANN segment path has no parent"))?;
        fs::create_dir_all(parent)?;
        let staging_path = parent.join(format!(
            ".{}.{}.tmp",
            hex_lower(&hash),
            NEXT_ANN_SEGMENT_STAGING_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut staging = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)?;
        let write_result = staging
            .write_all(&self.canonical_bytes())
            .and_then(|()| staging.sync_all());
        drop(staging);
        if let Err(error) = write_result {
            let _ = fs::remove_file(&staging_path);
            return Err(error.into());
        }
        if path.is_file() {
            let _ = fs::remove_file(&staging_path);
            verify_ann_segment_file(&path, hash)?;
            return Ok(hash);
        }
        match fs::rename(&staging_path, &path) {
            Ok(()) => {
                sync_directory(parent)?;
                Ok(hash)
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&staging_path);
                verify_ann_segment_file(&path, hash)?;
                Ok(hash)
            }
            Err(error) => {
                let _ = fs::remove_file(&staging_path);
                Err(error.into())
            }
        }
    }

    pub fn get(derived_dir: impl AsRef<Path>, hash: [u8; 32]) -> Result<Self, DerivedError> {
        let path = ann_segment_path(derived_dir.as_ref(), hash);
        let bytes = fs::read(path)?;
        let segment = Self::from_canonical_bytes(&bytes)?;
        if segment.hash() != hash {
            return Err(invalid_payload(
                "ANN segment checksum does not match its path",
            ));
        }
        Ok(segment)
    }

    pub fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        let synthetic_hits = self.index.search(query, limit, filter)?;
        Ok(synthetic_hits
            .into_iter()
            .filter_map(|hit| {
                self.synthetic_memberships
                    .get(&hit.membership())
                    .copied()
                    .map(|membership| VectorHit {
                        membership,
                        score: hit.score(),
                    })
            })
            .collect())
    }

    pub fn merge_search<'a>(
        segments: impl IntoIterator<Item = &'a AnnSegmentV1>,
        tombstones: &BTreeSet<String>,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        let mut seen = BTreeSet::new();
        let mut hits = Vec::new();
        for segment in segments {
            for entry in &segment.entries {
                if tombstones.contains(&entry.target_key) || !seen.insert(entry.target_key.clone())
                {
                    continue;
                }
                if !filter.matches(&entry.membership) {
                    continue;
                }
                let score = cosine_similarity(query, &entry.values)?;
                hits.push(VectorHit {
                    membership: entry.membership,
                    score,
                });
            }
        }
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.membership.cmp(&right.membership))
        });
        hits.truncate(limit);
        Ok(hits)
    }
}

fn put_string_u32(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
    bytes.extend_from_slice(value);
}

fn read_string_u32(bytes: &[u8], cursor: &mut usize) -> Result<String, DerivedError> {
    let length = usize::try_from(read_u32(bytes, cursor)?)
        .map_err(|_| invalid_payload("encoded ANN string length is out of range"))?;
    let end = cursor
        .checked_add(length)
        .ok_or_else(|| invalid_payload("encoded ANN string length overflow"))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or_else(|| invalid_payload("encoded ANN string is truncated"))?;
    *cursor = end;
    String::from_utf8(value.to_vec())
        .map_err(|_| invalid_payload("encoded ANN string is not UTF-8"))
}

fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, DerivedError> {
    Ok(u64::from_le_bytes(read_fixed(bytes, cursor)?))
}

fn ann_segment_path(derived_dir: &Path, hash: [u8; 32]) -> PathBuf {
    let hex = hex_lower(&hash);
    derived_dir
        .join("objects")
        .join("ann")
        .join(&hex[..2])
        .join(format!("{hex}.usearch"))
}

#[must_use]
pub fn should_schedule_ann_compaction(
    delta_segment_count: usize,
    tombstone_count: u64,
    live_target_count: u64,
) -> bool {
    if delta_segment_count > ANN_DELTA_COMPACTION_THRESHOLD {
        return true;
    }
    let total_targets = tombstone_count.saturating_add(live_target_count);
    total_targets > 0
        && u128::from(tombstone_count) * 100
            > u128::from(total_targets) * u128::from(ANN_TOMBSTONE_RATIO_PERCENT)
}

fn verify_ann_segment_file(path: &Path, expected: [u8; 32]) -> Result<(), DerivedError> {
    let bytes = fs::read(path)?;
    let segment = AnnSegmentV1::from_canonical_bytes(&bytes)?;
    if segment.hash() != expected {
        return Err(invalid_payload("existing ANN segment checksum mismatch"));
    }
    Ok(())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> Result<f32, DerivedError> {
    if left.len() != right.len() {
        return Err(DerivedError::InvalidProjectionValue {
            value: "ANN query dimension mismatch".to_owned(),
        });
    }
    if left.iter().any(|value| !value.is_finite()) {
        return Err(DerivedError::InvalidProjectionValue {
            value: "ANN query values must be finite".to_owned(),
        });
    }
    let dot = left
        .iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        return Ok(0.0);
    }
    Ok(dot / (left_norm * right_norm))
}

/// An immutable, content-addressed vector payload.
///
/// The payload is independent from where a memory is currently attached. The
/// `Arc` keeps repeated memberships in the same in-process index from copying
/// the Rust-owned vector values.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorArtifact {
    signature: EmbeddingSignature,
    payload_hash: [u8; 32],
    values: Arc<[f32]>,
}

impl VectorArtifact {
    #[must_use]
    pub fn from_embedding(vector: EmbeddingVector) -> Self {
        let signature = vector.signature().clone();
        let payload_hash = vector.payload_hash();
        let values = Arc::<[f32]>::from(vector.values().to_vec().into_boxed_slice());
        Self {
            signature,
            payload_hash,
            values,
        }
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }

    #[must_use]
    pub const fn payload_hash(&self) -> &[u8; 32] {
        &self.payload_hash
    }

    #[must_use]
    pub fn values(&self) -> &[f32] {
        &self.values
    }
}

/// The authority-scoped membership of a vector payload.
///
/// Moving a memory creates a new membership while preserving the immutable
/// payload hash. The generation is part of the membership so stale derived
/// rows cannot be mistaken for the current authority view.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VectorMembership {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    authority_generation: AuthorityGeneration,
    payload_hash: [u8; 32],
}

impl VectorMembership {
    #[must_use]
    pub const fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        authority_generation: AuthorityGeneration,
        artifact: &VectorArtifact,
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            authority_generation,
            payload_hash: *artifact.payload_hash(),
        }
    }

    #[must_use]
    pub const fn from_parts(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        authority_generation: AuthorityGeneration,
        payload_hash: [u8; 32],
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            authority_generation,
            payload_hash,
        }
    }

    #[must_use]
    pub const fn space_id(self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub const fn memory_id(self) -> MemoryId {
        self.memory_id
    }

    #[must_use]
    pub const fn revision_id(self) -> RevisionId {
        self.revision_id
    }

    #[must_use]
    pub const fn authority_generation(self) -> AuthorityGeneration {
        self.authority_generation
    }

    #[must_use]
    pub const fn payload_hash(self) -> [u8; 32] {
        self.payload_hash
    }

    #[must_use]
    fn identity(self) -> VectorMembershipIdentity {
        VectorMembershipIdentity {
            space_id: self.space_id,
            memory_id: self.memory_id,
            revision_id: self.revision_id,
            authority_generation: self.authority_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VectorFilter {
    space_id: Option<SpaceId>,
    memory_id: Option<MemoryId>,
    revision_id: Option<RevisionId>,
    authority_generation: Option<AuthorityGeneration>,
}

impl VectorFilter {
    #[must_use]
    pub const fn any() -> Self {
        Self {
            space_id: None,
            memory_id: None,
            revision_id: None,
            authority_generation: None,
        }
    }

    #[must_use]
    pub const fn with_space_id(mut self, space_id: SpaceId) -> Self {
        self.space_id = Some(space_id);
        self
    }

    #[must_use]
    pub const fn with_memory_id(mut self, memory_id: MemoryId) -> Self {
        self.memory_id = Some(memory_id);
        self
    }

    #[must_use]
    pub const fn with_revision_id(mut self, revision_id: RevisionId) -> Self {
        self.revision_id = Some(revision_id);
        self
    }

    #[must_use]
    pub const fn with_authority_generation(
        mut self,
        authority_generation: AuthorityGeneration,
    ) -> Self {
        self.authority_generation = Some(authority_generation);
        self
    }

    #[must_use]
    fn matches(self, membership: &VectorMembership) -> bool {
        self.space_id
            .is_none_or(|value| value == membership.space_id)
            && self
                .memory_id
                .is_none_or(|value| value == membership.memory_id)
            && self
                .revision_id
                .is_none_or(|value| value == membership.revision_id)
            && self
                .authority_generation
                .is_none_or(|value| value == membership.authority_generation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorHit {
    membership: VectorMembership,
    score: f32,
}

impl VectorHit {
    #[must_use]
    pub const fn membership(self) -> VectorMembership {
        self.membership
    }

    #[must_use]
    pub const fn score(self) -> f32 {
        self.score
    }
}

/// Provider-neutral vector search boundary.
///
/// Implementations own the ANN choice and must apply authority membership
/// filters before returning hits.
pub trait VectorSearch {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError>;
}

/// An immutable-payload vector index with authority-scoped memberships.
pub struct VectorIndex {
    signature: EmbeddingSignature,
    adapter: UsearchVectorSearch,
}

impl VectorIndex {
    pub fn new(signature: EmbeddingSignature) -> Result<Self, DerivedError> {
        let options = IndexOptions {
            dimensions: signature.dimensions(),
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            ..Default::default()
        };
        let index = Index::new(&options).map_err(vector_index_error)?;
        Ok(Self {
            signature,
            adapter: UsearchVectorSearch::new(index),
        })
    }

    #[must_use]
    pub const fn signature(&self) -> &EmbeddingSignature {
        &self.signature
    }

    #[must_use]
    pub fn payload_count(&self) -> usize {
        self.adapter.payloads.len()
    }

    #[must_use]
    pub fn membership_count(&self) -> usize {
        self.adapter.memberships.len()
    }

    pub fn insert(
        &mut self,
        artifact: VectorArtifact,
        membership: VectorMembership,
    ) -> Result<(), DerivedError> {
        if artifact.signature() != self.signature() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector artifact signature does not match index signature".to_owned(),
            });
        }
        if membership.payload_hash() != *artifact.payload_hash() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector membership payload hash does not match artifact".to_owned(),
            });
        }
        self.adapter.insert(artifact, membership)
    }

    pub fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        self.adapter.search(query, limit, filter)
    }
}

impl VectorSearch for VectorIndex {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        self.search(query, limit, filter)
    }
}

struct UsearchVectorSearch {
    index: Index,
    memberships: BTreeMap<u64, VectorMembership>,
    membership_keys: BTreeMap<VectorMembershipIdentity, u64>,
    payloads: BTreeMap<[u8; 32], VectorArtifact>,
    next_key: u64,
}

impl UsearchVectorSearch {
    fn new(index: Index) -> Self {
        Self {
            index,
            memberships: BTreeMap::new(),
            membership_keys: BTreeMap::new(),
            payloads: BTreeMap::new(),
            next_key: 1,
        }
    }

    fn insert(
        &mut self,
        artifact: VectorArtifact,
        membership: VectorMembership,
    ) -> Result<(), DerivedError> {
        if let Some(key) = self.membership_keys.get(&membership.identity()) {
            let existing = self
                .memberships
                .get(key)
                .ok_or_else(|| vector_index_error("membership index is internally inconsistent"))?;
            if existing.payload_hash() != membership.payload_hash() {
                return Err(DerivedError::InvalidProjectionValue {
                    value: "vector membership already points to a different payload".to_owned(),
                });
            }
            return Ok(());
        }

        if let Some(existing) = self.payloads.get(artifact.payload_hash())
            && existing != &artifact
        {
            return Err(DerivedError::InvalidProjectionValue {
                value: "vector payload hash collision detected".to_owned(),
            });
        }

        let key = self.next_key;
        self.next_key = self
            .next_key
            .checked_add(1)
            .ok_or_else(|| vector_index_error("vector membership key space exhausted"))?;
        if self.index.capacity() <= self.memberships.len() {
            let capacity = self
                .memberships
                .len()
                .max(1)
                .checked_mul(2)
                .ok_or_else(|| vector_index_error("vector index capacity exhausted"))?;
            self.index.reserve(capacity).map_err(vector_index_error)?;
        }
        self.index
            .add(key, artifact.values())
            .map_err(vector_index_error)?;
        self.payloads
            .entry(*artifact.payload_hash())
            .or_insert_with(|| artifact.clone());
        self.memberships.insert(key, membership);
        self.membership_keys.insert(membership.identity(), key);
        Ok(())
    }
}

impl VectorSearch for UsearchVectorSearch {
    fn search(
        &self,
        query: &[f32],
        limit: usize,
        filter: &VectorFilter,
    ) -> Result<Vec<VectorHit>, DerivedError> {
        validate_query(self.index.dimensions(), query)?;
        if limit == 0 || self.memberships.is_empty() {
            return Ok(Vec::new());
        }

        let count = limit.min(self.memberships.len());
        let matches = self
            .index
            .filtered_search(query, count, |key| {
                self.memberships
                    .get(&key)
                    .is_some_and(|membership| filter.matches(membership))
            })
            .map_err(vector_index_error)?;

        let mut hits = matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .filter_map(|(key, distance)| {
                self.memberships.get(key).map(|membership| VectorHit {
                    membership: *membership,
                    score: 1.0 - distance,
                })
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.membership.cmp(&right.membership))
        });
        Ok(hits)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct VectorMembershipIdentity {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    authority_generation: AuthorityGeneration,
}

fn validate_query(dimensions: usize, query: &[f32]) -> Result<(), DerivedError> {
    if query.len() != dimensions {
        return Err(DerivedError::InvalidProjectionValue {
            value: format!(
                "vector query dimension mismatch: expected {dimensions}, got {}",
                query.len()
            ),
        });
    }
    if query.iter().any(|value| !value.is_finite()) {
        return Err(DerivedError::InvalidProjectionValue {
            value: "vector query values must be finite".to_owned(),
        });
    }
    let norm = query.iter().map(|value| value * value).sum::<f32>();
    if norm == 0.0 {
        return Err(DerivedError::InvalidProjectionValue {
            value: "vector query must not be zero".to_owned(),
        });
    }
    Ok(())
}

fn vector_index_error(error: impl std::fmt::Display) -> DerivedError {
    DerivedError::VectorIndex {
        value: error.to_string(),
    }
}
