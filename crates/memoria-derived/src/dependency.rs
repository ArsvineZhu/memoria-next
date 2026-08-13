use std::collections::BTreeSet;

use memoria_mdx::{InvalidationCategory, SemanticDiff};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProjectionKind {
    Ir,
    Structural,
    Temporal,
    Relations,
    EntityObservations,
    ExplicitTags,
    Lexical,
    LocalEmbedding,
    ContextEmbedding,
    GeneratedTags,
    TagGraph,
    RerankView,
}

impl ProjectionKind {
    pub const ALL: &'static [Self] = &[
        Self::Ir,
        Self::Structural,
        Self::Temporal,
        Self::Relations,
        Self::EntityObservations,
        Self::ExplicitTags,
        Self::Lexical,
        Self::LocalEmbedding,
        Self::ContextEmbedding,
        Self::GeneratedTags,
        Self::TagGraph,
        Self::RerankView,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ir => "ir",
            Self::Structural => "structural",
            Self::Temporal => "temporal",
            Self::Relations => "relations",
            Self::EntityObservations => "entity-observations",
            Self::ExplicitTags => "explicit-tags",
            Self::Lexical => "lexical",
            Self::LocalEmbedding => "local-embedding",
            Self::ContextEmbedding => "context-embedding",
            Self::GeneratedTags => "generated-tags",
            Self::TagGraph => "tag-graph",
            Self::RerankView => "rerank-view",
        }
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        1
    }

    #[must_use]
    pub const fn dependencies(self) -> &'static [Self] {
        match self {
            Self::Ir => &[],
            Self::Structural | Self::Temporal | Self::Relations | Self::Lexical => &[Self::Ir],
            Self::EntityObservations => &[Self::Ir, Self::Structural],
            Self::ExplicitTags => &[Self::Ir, Self::Structural],
            Self::LocalEmbedding => &[Self::Ir],
            Self::ContextEmbedding => &[Self::Ir, Self::Structural],
            Self::GeneratedTags => &[Self::LocalEmbedding, Self::ContextEmbedding],
            Self::TagGraph => &[Self::ExplicitTags, Self::GeneratedTags],
            Self::RerankView => &[
                Self::Structural,
                Self::Temporal,
                Self::Relations,
                Self::EntityObservations,
                Self::ExplicitTags,
                Self::Lexical,
                Self::LocalEmbedding,
                Self::ContextEmbedding,
                Self::GeneratedTags,
                Self::TagGraph,
            ],
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectionInputHash([u8; 32]);

impl ProjectionInputHash {
    #[must_use]
    pub fn new(
        kind: ProjectionKind,
        canonical_projection_bytes: &[u8],
        producer_signature: &str,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-projection-input-v1\0");
        put_string(&mut hasher, kind.as_str().as_bytes());
        hasher.update(kind.version().to_be_bytes());
        put_string(&mut hasher, producer_signature.as_bytes());
        put_string(&mut hasher, canonical_projection_bytes);
        Self(hasher.finalize().into())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvalidationPlan {
    rebuilds: BTreeSet<ProjectionKind>,
}

impl InvalidationPlan {
    #[must_use]
    pub fn from_diff(diff: &SemanticDiff) -> Self {
        let mut plan = Self::default();
        if diff.is_empty() {
            return plan;
        }
        plan.rebuilds.insert(ProjectionKind::Ir);
        for category in [
            InvalidationCategory::VisibleText,
            InvalidationCategory::Hierarchy,
            InvalidationCategory::EntityBinding,
            InvalidationCategory::EntitySurface,
            InvalidationCategory::ExplicitTag,
            InvalidationCategory::Temporal,
            InvalidationCategory::Relation,
            InvalidationCategory::MemoryRef,
            InvalidationCategory::SourceMetadata,
            InvalidationCategory::Extension,
            InvalidationCategory::SemanticNodeLifecycle,
        ] {
            if diff.has_category(category) {
                plan.add_category(category);
            }
        }
        plan
    }

    #[must_use]
    pub fn rebuilds(&self, kind: ProjectionKind) -> bool {
        self.rebuilds.contains(&kind)
    }

    pub fn projections(&self) -> impl Iterator<Item = ProjectionKind> + '_ {
        self.rebuilds.iter().copied()
    }

    #[must_use]
    pub fn input_hash(
        &self,
        kind: ProjectionKind,
        canonical_projection_bytes: &[u8],
        producer_signature: &str,
    ) -> ProjectionInputHash {
        ProjectionInputHash::new(kind, canonical_projection_bytes, producer_signature)
    }

    fn add_category(&mut self, category: InvalidationCategory) {
        let roots: &[ProjectionKind] = match category {
            InvalidationCategory::VisibleText => &[
                ProjectionKind::Lexical,
                ProjectionKind::LocalEmbedding,
                ProjectionKind::ContextEmbedding,
            ],
            InvalidationCategory::Hierarchy => &[ProjectionKind::Structural],
            InvalidationCategory::EntityBinding => &[
                ProjectionKind::Structural,
                ProjectionKind::EntityObservations,
                ProjectionKind::ContextEmbedding,
            ],
            InvalidationCategory::EntitySurface => &[
                ProjectionKind::EntityObservations,
                ProjectionKind::Lexical,
                ProjectionKind::LocalEmbedding,
                ProjectionKind::ContextEmbedding,
            ],
            InvalidationCategory::ExplicitTag => &[ProjectionKind::ExplicitTags],
            InvalidationCategory::Temporal => &[ProjectionKind::Temporal],
            InvalidationCategory::Relation => &[ProjectionKind::Relations],
            InvalidationCategory::MemoryRef => &[ProjectionKind::Structural],
            InvalidationCategory::SourceMetadata => &[ProjectionKind::Structural],
            InvalidationCategory::Extension => &[ProjectionKind::Structural],
            InvalidationCategory::SemanticNodeLifecycle => &[ProjectionKind::Structural],
        };
        for root in roots {
            self.add_with_dependents(*root);
        }
    }

    fn add_with_dependents(&mut self, root: ProjectionKind) {
        if !self.rebuilds.insert(root) {
            return;
        }
        for dependent in ProjectionKind::ALL {
            if dependent.dependencies().contains(&root) {
                self.add_with_dependents(*dependent);
            }
        }
    }
}

fn put_string(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
