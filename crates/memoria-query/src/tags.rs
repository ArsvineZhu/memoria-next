use std::collections::{BTreeMap, BTreeSet};

use memoria_derived::{TagGraph, TagId, TagProvenance, TagSpaceGraph};
use memoria_types::{MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};

use crate::validate::QueryError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TagSeedProvenance {
    Explicit,
    ExactSupport,
    Semantic,
    Generated,
    Inherited,
}

impl TagSeedProvenance {
    #[must_use]
    pub const fn weight(self) -> f32 {
        match self {
            Self::Explicit => 1.00,
            Self::ExactSupport => 0.95,
            Self::Semantic => 0.70,
            Self::Generated => 0.55,
            Self::Inherited => 0.40,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagSeed {
    pub tag_id: TagId,
    pub value: String,
    pub provenance: TagSeedProvenance,
}

impl TagSeed {
    #[must_use]
    pub const fn weight(&self) -> f32 {
        self.provenance.weight()
    }
}

/// A query-local Tag vector candidate used by the Tag Basis operator.
///
/// The vector is discovery evidence only; it never changes the authoritative
/// Tag dictionary or membership tables.
#[derive(Clone, Debug, PartialEq)]
pub struct TagVectorCandidate {
    pub tag_id: TagId,
    pub vector: Vec<f32>,
    pub provenance: TagSeedProvenance,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TagReadoutCandidate {
    pub target: crate::CandidateTarget,
    pub tag_id: TagId,
    pub node_id: Option<String>,
    pub provenance: TagProvenance,
    pub score: f32,
}

/// Read Tag memberships into revision-pinned query candidates.
///
/// This function only reads the supplied Space-local graph. It never merges
/// the composite view back into the graph and never expands the caller's
/// authorized Space set.
#[must_use]
pub fn readout_tag_candidates(
    graph: &TagGraph,
    seed_ids: &[TagId],
    allowed_spaces: &[SpaceId],
) -> Vec<TagReadoutCandidate> {
    let seeds = seed_ids.iter().copied().collect::<BTreeSet<_>>();
    let spaces = allowed_spaces.iter().copied().collect::<BTreeSet<_>>();
    let mut candidates = graph
        .membership_evidence()
        .filter(|evidence| seeds.contains(&evidence.tag_id) && spaces.contains(&evidence.space_id))
        .map(|evidence| TagReadoutCandidate {
            target: crate::CandidateTarget {
                space_id: evidence.space_id,
                memory_id: evidence.memory_id,
                revision_id: evidence.revision_id,
            },
            tag_id: evidence.tag_id,
            node_id: evidence.node_id.clone(),
            provenance: evidence.provenance,
            score: match evidence.provenance {
                TagProvenance::Explicit => TagSeedProvenance::ExactSupport.weight(),
                TagProvenance::Generated => TagSeedProvenance::Generated.weight(),
            },
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.target.space_id.cmp(&right.target.space_id))
            .then_with(|| left.target.memory_id.cmp(&right.target.memory_id))
            .then_with(|| left.target.revision_id.cmp(&right.target.revision_id))
            .then_with(|| left.tag_id.cmp(&right.tag_id))
    });
    candidates
}

/// Select and deterministically order Tag vectors for a bounded basis.
///
/// Explicit cue Tags are ordered before semantic neighbors. Semantic
/// neighbors are ignored unless the semantic capability is active. Ties are
/// resolved by TagId so the same query snapshot always gets the same basis.
#[must_use]
pub fn select_tag_basis_candidates(
    candidates: impl IntoIterator<Item = TagVectorCandidate>,
    semantic_active: bool,
    limit: usize,
) -> Vec<TagVectorCandidate> {
    let mut selected = candidates
        .into_iter()
        .filter(|candidate| semantic_active || candidate.provenance != TagSeedProvenance::Semantic)
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        left.provenance
            .cmp(&right.provenance)
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.tag_id.cmp(&right.tag_id))
    });
    selected.truncate(limit.min(crate::MAX_TAG_BASIS_VECTORS));
    selected
}

/// Resolve deterministic explicit Tag cues through the global dictionary.
pub fn resolve_explicit_tag_seeds<I, S>(
    dictionary: &mut memoria_derived::TagDictionary,
    values: I,
) -> Result<Vec<TagSeed>, QueryError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = BTreeSet::new();
    let mut seeds = Vec::new();
    for value in values {
        let tag_id =
            dictionary
                .intern(value.as_ref())
                .map_err(|error| QueryError::InvalidTagCue {
                    value: error.to_string(),
                })?;
        if seen.insert(tag_id) {
            let normalized = dictionary
                .value(tag_id)
                .ok_or_else(|| QueryError::InvalidTagCue {
                    value: "Tag dictionary lost the resolved identity".to_owned(),
                })?;
            seeds.push(TagSeed {
                tag_id,
                value: normalized.to_owned(),
                provenance: TagSeedProvenance::Explicit,
            });
        }
    }
    Ok(seeds)
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompositeTagAssociation {
    pub left: TagId,
    pub right: TagId,
    explicit_evidence: u32,
    generated_evidence: u32,
    weight: f32,
}

impl CompositeTagAssociation {
    #[must_use]
    pub const fn explicit_evidence(&self) -> u32 {
        self.explicit_evidence
    }

    #[must_use]
    pub const fn generated_evidence(&self) -> u32 {
        self.generated_evidence
    }

    #[must_use]
    pub const fn weight(&self) -> f32 {
        self.weight
    }
}

/// A query-local, immutable composite of explicitly scoped Space graphs.
///
/// The constructor copies only bounded aggregate evidence. No write-back path
/// exists from this view to the Derived graph, which keeps cross-Space query
/// composition from contaminating Space-local artifacts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CompositeTagView {
    spaces: Vec<SpaceId>,
    memberships: BTreeMap<TagId, usize>,
    edges: BTreeMap<(TagId, TagId), CompositeTagAssociation>,
}

impl CompositeTagView {
    #[must_use]
    pub fn from_scope<I>(graph: &TagGraph, scope: I) -> Self
    where
        I: IntoIterator<Item = SpaceId>,
    {
        let mut spaces = scope.into_iter().collect::<Vec<_>>();
        spaces.sort_unstable();
        spaces.dedup();

        let mut membership_targets =
            BTreeMap::<TagId, BTreeSet<(SpaceId, MemoryId, RevisionId)>>::new();
        let mut edges = BTreeMap::<(TagId, TagId), CompositeTagAssociation>::new();
        for space_id in &spaces {
            let space = graph.for_space(*space_id);
            add_memberships(&mut membership_targets, space);
            for edge in space.edges() {
                let key = ordered_pair(edge.left, edge.right);
                let aggregate = edges.entry(key).or_insert(CompositeTagAssociation {
                    left: key.0,
                    right: key.1,
                    explicit_evidence: 0,
                    generated_evidence: 0,
                    weight: 0.0,
                });
                aggregate.explicit_evidence += edge.explicit_evidence();
                aggregate.generated_evidence += edge.generated_evidence();
                aggregate.weight =
                    aggregate.explicit_evidence as f32 + aggregate.generated_evidence as f32 * 0.5;
            }
        }

        Self {
            spaces,
            memberships: membership_targets
                .into_iter()
                .map(|(tag_id, targets)| (tag_id, targets.len()))
                .collect(),
            edges,
        }
    }

    #[must_use]
    pub fn spaces(&self) -> &[SpaceId] {
        &self.spaces
    }

    #[must_use]
    pub fn membership_count(&self, tag_id: TagId) -> usize {
        self.memberships.get(&tag_id).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn edge(&self, left: TagId, right: TagId) -> Option<&CompositeTagAssociation> {
        let key = ordered_pair(left, right);
        self.edges.get(&key)
    }

    pub fn associations(&self) -> impl Iterator<Item = &CompositeTagAssociation> {
        self.edges.values()
    }

    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-composite-tag-view-v1\0");
        for space in &self.spaces {
            hasher.update(space.as_bytes());
        }
        for (tag_id, count) in &self.memberships {
            hasher.update(tag_id.as_bytes());
            hasher.update((*count as u64).to_be_bytes());
        }
        for edge in self.edges.values() {
            hasher.update(edge.left.as_bytes());
            hasher.update(edge.right.as_bytes());
            hasher.update(edge.explicit_evidence.to_be_bytes());
            hasher.update(edge.generated_evidence.to_be_bytes());
            hasher.update(edge.weight.to_bits().to_be_bytes());
        }
        hasher.finalize().into()
    }
}

fn add_memberships(
    targets: &mut BTreeMap<TagId, BTreeSet<(SpaceId, MemoryId, RevisionId)>>,
    space: TagSpaceGraph<'_>,
) {
    for evidence in space.memberships() {
        targets.entry(evidence.tag_id).or_default().insert((
            evidence.space_id,
            evidence.memory_id,
            evidence.revision_id,
        ));
    }
}

fn ordered_pair(left: TagId, right: TagId) -> (TagId, TagId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
