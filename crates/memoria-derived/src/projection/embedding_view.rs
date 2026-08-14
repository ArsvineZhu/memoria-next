use memoria_mdx::{IrNode, MemoryIr, SemanticKind};

use crate::DerivedError;
use crate::dependency::{ProjectionInputHash, ProjectionKind};

pub const LOCAL_EMBEDDING_PROJECTION_VERSION: u32 = 1;
pub const CONTEXT_EMBEDDING_PROJECTION_VERSION: u32 = 1;
pub const QUERY_EMBEDDING_PROJECTION_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryEmbeddingProjectionV1 {
    content: String,
}

impl QueryEmbeddingProjectionV1 {
    #[must_use]
    pub fn build(text: impl AsRef<str>) -> Self {
        Self {
            content: normalize_whitespace(text.as_ref()),
        }
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn version() -> u32 {
        QUERY_EMBEDDING_PROJECTION_VERSION
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalEmbeddingProjectionV1 {
    content: String,
    input_hash: ProjectionInputHash,
    producer_signature: String,
}

impl LocalEmbeddingProjectionV1 {
    pub fn build(
        ir: &MemoryIr,
        producer_signature: impl Into<String>,
    ) -> Result<Self, DerivedError> {
        build_projection(
            ir,
            producer_signature,
            ProjectionKind::LocalEmbedding,
            local_embedding_text,
        )
        .map(|(content, input_hash, producer_signature)| Self {
            content,
            input_hash,
            producer_signature,
        })
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn input_hash(&self) -> &ProjectionInputHash {
        &self.input_hash
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }

    #[must_use]
    pub const fn version() -> u32 {
        LOCAL_EMBEDDING_PROJECTION_VERSION
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextEmbeddingProjectionV1 {
    content: String,
    input_hash: ProjectionInputHash,
    producer_signature: String,
}

impl ContextEmbeddingProjectionV1 {
    pub fn build(
        ir: &MemoryIr,
        producer_signature: impl Into<String>,
    ) -> Result<Self, DerivedError> {
        build_projection(
            ir,
            producer_signature,
            ProjectionKind::ContextEmbedding,
            context_embedding_text,
        )
        .map(|(content, input_hash, producer_signature)| Self {
            content,
            input_hash,
            producer_signature,
        })
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn input_hash(&self) -> &ProjectionInputHash {
        &self.input_hash
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }

    #[must_use]
    pub const fn version() -> u32 {
        CONTEXT_EMBEDDING_PROJECTION_VERSION
    }
}

pub(crate) fn context_embedding_text(ir: &MemoryIr) -> String {
    let mut output = ir.text_hierarchy().to_owned();
    for node in ir.nodes().filter(|node| node.kind() != SemanticKind::Tag) {
        append_text(&mut output, node);
    }
    output
}

fn local_embedding_text(ir: &MemoryIr) -> String {
    let mut output = String::new();
    for line in ir.text_hierarchy().lines().filter(|line| !is_heading(line)) {
        append_segment(&mut output, line);
    }
    for node in ir.nodes().filter(|node| node.kind() != SemanticKind::Tag) {
        append_text(&mut output, node);
    }
    output
}

fn build_projection(
    ir: &MemoryIr,
    producer_signature: impl Into<String>,
    kind: ProjectionKind,
    content_builder: impl FnOnce(&MemoryIr) -> String,
) -> Result<(String, ProjectionInputHash, String), DerivedError> {
    let producer_signature = producer_signature.into();
    if producer_signature.trim().is_empty() {
        return Err(DerivedError::InvalidProjectionValue {
            value: "embedding producer signature must not be empty".to_owned(),
        });
    }
    let content = content_builder(ir);
    let bytes = canonical_projection_bytes(&content);
    let input_hash = ProjectionInputHash::new(kind, &bytes, &producer_signature);
    Ok((content, input_hash, producer_signature))
}

fn canonical_projection_bytes(content: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(content.len() + 16);
    bytes.extend_from_slice(b"memoria-provider-projection-v1\0");
    put_string(&mut bytes, content);
    bytes
}

fn append_text(output: &mut String, node: &IrNode) {
    append_segment(output, node.text());
}

fn append_segment(output: &mut String, value: &str) {
    if value.is_empty() {
        return;
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(value);
}

fn is_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    hashes > 0
        && hashes <= 6
        && trimmed
            .as_bytes()
            .get(hashes)
            .is_none_or(|byte| *byte == b' ')
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
