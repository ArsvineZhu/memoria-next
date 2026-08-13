use std::ops::Range;

use crate::profile::{SemanticKind, SemanticNodeId};
use crate::time::TemporalValue;

pub const MEMORY_IR_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticHash([u8; 32]);

impl SemanticHash {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut output = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        }
        output
    }
}

impl AsRef<[u8]> for SemanticHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Display for SemanticHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapping {
    kind: SemanticKind,
    id: Option<SemanticNodeId>,
    span: Range<usize>,
}

impl SourceMapping {
    pub(crate) fn new(kind: SemanticKind, id: Option<SemanticNodeId>, span: Range<usize>) -> Self {
        Self { kind, id, span }
    }

    #[must_use]
    pub fn kind(&self) -> SemanticKind {
        self.kind
    }

    #[must_use]
    pub fn id(&self) -> Option<&SemanticNodeId> {
        self.id.as_ref()
    }

    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IrNode {
    kind: SemanticKind,
    id: Option<SemanticNodeId>,
    attributes: Vec<(String, String)>,
    text: String,
    occurred_at: Option<TemporalValue>,
    observed_at: Option<TemporalValue>,
    valid_from: Option<TemporalValue>,
    valid_to: Option<TemporalValue>,
    source_mapping: SourceMapping,
}

pub(crate) struct IrTemporalFields {
    pub(crate) occurred_at: Option<TemporalValue>,
    pub(crate) observed_at: Option<TemporalValue>,
    pub(crate) valid_from: Option<TemporalValue>,
    pub(crate) valid_to: Option<TemporalValue>,
}

impl IrNode {
    pub(crate) fn new(
        kind: SemanticKind,
        id: Option<SemanticNodeId>,
        attributes: Vec<(String, String)>,
        text: String,
        temporal: IrTemporalFields,
        source_mapping: SourceMapping,
    ) -> Self {
        Self {
            kind,
            id,
            attributes,
            text,
            occurred_at: temporal.occurred_at,
            observed_at: temporal.observed_at,
            valid_from: temporal.valid_from,
            valid_to: temporal.valid_to,
            source_mapping,
        }
    }

    #[must_use]
    pub fn kind(&self) -> SemanticKind {
        self.kind
    }

    #[must_use]
    pub fn id(&self) -> Option<&SemanticNodeId> {
        self.id.as_ref()
    }

    #[must_use]
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn occurred_at(&self) -> Option<&TemporalValue> {
        self.occurred_at.as_ref()
    }

    #[must_use]
    pub fn observed_at(&self) -> Option<&TemporalValue> {
        self.observed_at.as_ref()
    }

    #[must_use]
    pub fn valid_from(&self) -> Option<&TemporalValue> {
        self.valid_from.as_ref()
    }

    #[must_use]
    pub fn valid_to(&self) -> Option<&TemporalValue> {
        self.valid_to.as_ref()
    }

    #[must_use]
    pub fn source_mapping(&self) -> &SourceMapping {
        &self.source_mapping
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryIr {
    canonical_bytes: Vec<u8>,
    semantic_hash: SemanticHash,
    text_hierarchy: String,
    nodes: Vec<IrNode>,
    source_mappings: Vec<SourceMapping>,
}

impl MemoryIr {
    pub(crate) fn new(
        canonical_bytes: Vec<u8>,
        semantic_hash: SemanticHash,
        text_hierarchy: String,
        nodes: Vec<IrNode>,
        source_mappings: Vec<SourceMapping>,
    ) -> Self {
        Self {
            canonical_bytes,
            semantic_hash,
            text_hierarchy,
            nodes,
            source_mappings,
        }
    }

    #[must_use]
    pub fn version(&self) -> u16 {
        MEMORY_IR_VERSION
    }

    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    #[must_use]
    pub fn semantic_hash(&self) -> &SemanticHash {
        &self.semantic_hash
    }

    #[must_use]
    pub fn text_hierarchy(&self) -> &str {
        &self.text_hierarchy
    }

    pub fn nodes(&self) -> impl Iterator<Item = &IrNode> {
        self.nodes.iter()
    }

    pub fn source_mappings(&self) -> impl Iterator<Item = &SourceMapping> {
        self.source_mappings.iter()
    }

    #[must_use]
    pub fn node(&self, id: &SemanticNodeId) -> Option<&IrNode> {
        self.nodes.iter().find(|node| node.id() == Some(id))
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";
