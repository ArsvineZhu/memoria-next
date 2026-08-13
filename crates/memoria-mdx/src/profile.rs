use std::{collections::BTreeMap, fmt, str::FromStr};

use crate::syntax::MdxError;
use crate::time::TemporalValue;
use crate::{ParsedSource, SemanticElement};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticNodeId(String);

impl SemanticNodeId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SemanticNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for SemanticNodeId {
    type Err = MdxError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty()
            || !value.as_bytes()[0].is_ascii_alphabetic()
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':')
            })
        {
            return Err(MdxError::InvalidNodeId {
                value: value.to_owned(),
                span: 0..value.len(),
            });
        }
        Ok(Self(value.to_owned()))
    }
}

pub type NodeId = SemanticNodeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticKind {
    MemoryMeta,
    Section,
    Entity,
    Tag,
    State,
    Event,
    Relation,
    MemoryRef,
    Source,
    Quote,
    Extension,
}

impl SemanticKind {
    pub(crate) fn parse(element: &SemanticElement) -> Self {
        match element.name() {
            "MemoryMeta" => Self::MemoryMeta,
            "Section" => Self::Section,
            "Entity" => Self::Entity,
            "Tag" => Self::Tag,
            "State" => Self::State,
            "Event" => Self::Event,
            "Relation" => Self::Relation,
            "MemoryRef" => Self::MemoryRef,
            "Source" => Self::Source,
            "Quote" => Self::Quote,
            "Extension" => Self::Extension,
            _ => unreachable!("semantic lexer only emits Core elements"),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MemoryMeta => "MemoryMeta",
            Self::Section => "Section",
            Self::Entity => "Entity",
            Self::Tag => "Tag",
            Self::State => "State",
            Self::Event => "Event",
            Self::Relation => "Relation",
            Self::MemoryRef => "MemoryRef",
            Self::Source => "Source",
            Self::Quote => "Quote",
            Self::Extension => "Extension",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedNode {
    kind: SemanticKind,
    id: Option<SemanticNodeId>,
    attributes: BTreeMap<String, String>,
    text: String,
    span: std::ops::Range<usize>,
    occurred_at: Option<TemporalValue>,
    observed_at: Option<TemporalValue>,
    valid_from: Option<TemporalValue>,
    valid_to: Option<TemporalValue>,
}

impl ValidatedNode {
    #[must_use]
    pub fn kind(&self) -> SemanticKind {
        self.kind
    }

    #[must_use]
    pub fn id(&self) -> Option<&SemanticNodeId> {
        self.id.as_ref()
    }

    #[must_use]
    pub fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn span(&self) -> std::ops::Range<usize> {
        self.span.clone()
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        kind: SemanticKind,
        id: Option<SemanticNodeId>,
        attributes: BTreeMap<String, String>,
        text: String,
        span: std::ops::Range<usize>,
        occurred_at: Option<TemporalValue>,
        observed_at: Option<TemporalValue>,
        valid_from: Option<TemporalValue>,
        valid_to: Option<TemporalValue>,
    ) -> Self {
        Self {
            kind,
            id,
            attributes,
            text,
            span,
            occurred_at,
            observed_at,
            valid_from,
            valid_to,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedDocument {
    parsed: ParsedSource,
    nodes: Vec<ValidatedNode>,
}

impl ValidatedDocument {
    pub(crate) fn new(parsed: ParsedSource, nodes: Vec<ValidatedNode>) -> Self {
        Self { parsed, nodes }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        self.parsed.source()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &ValidatedNode> {
        self.nodes.iter()
    }

    #[must_use]
    pub fn node(&self, id: &SemanticNodeId) -> Option<&ValidatedNode> {
        self.nodes.iter().find(|node| node.id.as_ref() == Some(id))
    }

    #[must_use]
    pub fn event(&self, id: &str) -> Option<&ValidatedNode> {
        self.find(SemanticKind::Event, id)
    }

    #[must_use]
    pub fn state(&self, id: &str) -> Option<&ValidatedNode> {
        self.find(SemanticKind::State, id)
    }

    fn find(&self, kind: SemanticKind, id: &str) -> Option<&ValidatedNode> {
        self.nodes.iter().find(|node| {
            node.kind == kind
                && node
                    .id
                    .as_ref()
                    .is_some_and(|node_id| node_id.as_str() == id)
        })
    }
}
