use std::collections::BTreeMap;

use memoria_mdx::{IrNode, MemoryIr, SemanticKind};
use memoria_types::{MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};

use crate::dependency::{ProjectionInputHash, ProjectionKind};
use crate::projection::ProjectionTarget;
use crate::projection::tags::TagProvenance;
use crate::{DerivedError, LexicalDocument, TagDictionary, TagId};

pub const ENRICHMENT_PROJECTION_VERSION: u32 = 1;
pub const DEFAULT_MAX_GENERATED_TAGS: usize = 8;

/// A versioned, provider-safe projection. Its content is canonical visible
/// text and node metadata, never the original MDX source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrichmentProjection {
    version: u32,
    input_hash: ProjectionInputHash,
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    semantic_node_id: Option<String>,
    content: String,
    max_tags: usize,
    producer_signature: String,
}

impl EnrichmentProjection {
    pub fn from_document(
        document: &LexicalDocument,
        producer_signature: impl Into<String>,
    ) -> Result<Self, DerivedError> {
        let producer_signature = producer_signature.into();
        if producer_signature.trim().is_empty() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "enrichment producer signature must not be empty".to_owned(),
            });
        }
        let content = enrichment_text(document.ir());
        let canonical_bytes = enrichment_projection_bytes(document.ir(), &content);
        let input_hash = ProjectionInputHash::new(
            ProjectionKind::GeneratedTags,
            &canonical_bytes,
            &producer_signature,
        );
        Ok(Self {
            version: ENRICHMENT_PROJECTION_VERSION,
            input_hash,
            space_id: document.space_id(),
            memory_id: document.memory_id(),
            revision_id: document.revision_id(),
            semantic_node_id: None,
            content,
            max_tags: DEFAULT_MAX_GENERATED_TAGS,
            producer_signature,
        })
    }

    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub const fn input_hash(&self) -> &ProjectionInputHash {
        &self.input_hash
    }

    #[must_use]
    pub const fn space_id(&self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub const fn memory_id(&self) -> MemoryId {
        self.memory_id
    }

    #[must_use]
    pub const fn revision_id(&self) -> RevisionId {
        self.revision_id
    }

    #[must_use]
    pub fn semantic_node_id(&self) -> Option<&str> {
        self.semantic_node_id.as_deref()
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn max_tags(&self) -> usize {
        self.max_tags
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }

    #[must_use]
    pub const fn target(&self) -> ProjectionTarget {
        ProjectionTarget {
            space_id: Some(self.space_id),
            memory_id: Some(self.memory_id),
            revision_id: Some(self.revision_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedTagCandidate {
    pub value: String,
    pub score: Option<f32>,
    pub confidence: Option<f32>,
}

impl GeneratedTagCandidate {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            score: None,
            confidence: None,
        }
    }

    #[must_use]
    pub const fn with_score(mut self, score: f32) -> Self {
        self.score = Some(score);
        self
    }

    #[must_use]
    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }
}

impl From<String> for GeneratedTagCandidate {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for GeneratedTagCandidate {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedTagRecord {
    pub tag_id: TagId,
    pub value: String,
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub semantic_node_id: Option<String>,
    pub producer_signature: String,
    pub projection_input_hash: ProjectionInputHash,
    pub score: Option<f32>,
    pub confidence: Option<f32>,
    pub provenance: TagProvenance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedTagArtifact {
    projection_input_hash: ProjectionInputHash,
    records: Vec<GeneratedTagRecord>,
    fingerprint: [u8; 32],
}

impl GeneratedTagArtifact {
    pub fn from_candidates<I, C>(
        projection: EnrichmentProjection,
        dictionary: &mut TagDictionary,
        candidates: I,
    ) -> Result<Self, DerivedError>
    where
        I: IntoIterator<Item = C>,
        C: Into<GeneratedTagCandidate>,
    {
        let candidates = candidates.into_iter().map(Into::into).collect::<Vec<_>>();
        if candidates.len() > projection.max_tags {
            return Err(DerivedError::InvalidProjectionValue {
                value: format!(
                    "generated Tag candidate count exceeds the limit of {}",
                    projection.max_tags
                ),
            });
        }
        let mut records = BTreeMap::<TagId, GeneratedTagRecord>::new();
        for candidate in candidates {
            validate_metric(candidate.score, "score")?;
            validate_metric(candidate.confidence, "confidence")?;
            let tag_id = dictionary.intern(&candidate.value)?;
            let value = dictionary
                .value(tag_id)
                .ok_or_else(|| DerivedError::InvalidProjectionValue {
                    value: "Tag dictionary lost the generated identity".to_owned(),
                })?
                .to_owned();
            records.entry(tag_id).or_insert_with(|| GeneratedTagRecord {
                tag_id,
                value,
                space_id: projection.space_id,
                memory_id: projection.memory_id,
                revision_id: projection.revision_id,
                semantic_node_id: projection.semantic_node_id.clone(),
                producer_signature: projection.producer_signature.clone(),
                projection_input_hash: projection.input_hash.clone(),
                score: candidate.score,
                confidence: candidate.confidence,
                provenance: TagProvenance::Generated,
            });
        }
        let records = records.into_values().collect::<Vec<_>>();
        let fingerprint = artifact_fingerprint(&projection.input_hash, &records);
        Ok(Self {
            projection_input_hash: projection.input_hash,
            records,
            fingerprint,
        })
    }

    #[must_use]
    pub const fn projection_input_hash(&self) -> &ProjectionInputHash {
        &self.projection_input_hash
    }

    #[must_use]
    pub fn records(&self) -> &[GeneratedTagRecord] {
        &self.records
    }

    #[must_use]
    pub const fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
}

fn validate_metric(value: Option<f32>, name: &str) -> Result<(), DerivedError> {
    if value.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(DerivedError::InvalidProjectionValue {
            value: format!("generated Tag {name} must be finite and between 0 and 1"),
        });
    }
    Ok(())
}

fn artifact_fingerprint(
    projection_input_hash: &ProjectionInputHash,
    records: &[GeneratedTagRecord],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"memoria-generated-tags-v1\0");
    hasher.update(projection_input_hash.as_bytes());
    for record in records {
        hasher.update(record.tag_id.as_bytes());
        hash_string(&mut hasher, record.value.as_bytes());
        hasher.update(record.space_id.as_bytes());
        hasher.update(record.memory_id.as_bytes());
        hasher.update(record.revision_id.as_bytes());
        hash_string(&mut hasher, record.producer_signature.as_bytes());
        if let Some(score) = record.score {
            hasher.update([1]);
            hasher.update(score.to_bits().to_be_bytes());
        } else {
            hasher.update([0]);
        }
        if let Some(confidence) = record.confidence {
            hasher.update([1]);
            hasher.update(confidence.to_bits().to_be_bytes());
        } else {
            hasher.update([0]);
        }
        hasher.update([1]);
    }
    hasher.finalize().into()
}

fn enrichment_text(ir: &MemoryIr) -> String {
    let mut output = ir.text_hierarchy().to_owned();
    for node in ir.nodes().filter(|node| node.kind() != SemanticKind::Tag) {
        append_text(&mut output, node);
    }
    output
}

fn append_text(output: &mut String, node: &IrNode) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(node.text());
}

fn enrichment_projection_bytes(ir: &MemoryIr, content: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    put_string(&mut bytes, content.as_bytes());
    for node in ir.nodes().filter(|node| node.kind() != SemanticKind::Tag) {
        put_string(&mut bytes, node.kind().as_str().as_bytes());
        put_string(
            &mut bytes,
            node.id()
                .map(ToString::to_string)
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
        );
        put_string(&mut bytes, node.text().as_bytes());
    }
    bytes
}

fn put_string(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn hash_string(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
