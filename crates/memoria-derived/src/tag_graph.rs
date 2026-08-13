use std::collections::{BTreeMap, BTreeSet};

use memoria_types::{MemoryId, RevisionId, SpaceId};
use sha2::{Digest, Sha256};

use crate::projection::tags::TagProvenance;
use crate::{DerivedError, TagDictionary, TagId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagMembershipInput {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub value: String,
    pub provenance: TagProvenance,
    pub node_id: Option<String>,
}

impl TagMembershipInput {
    #[must_use]
    pub fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        value: impl Into<String>,
        provenance: TagProvenance,
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            value: value.into(),
            provenance,
            node_id: None,
        }
    }

    #[must_use]
    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagMembershipEvidence {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub tag_id: TagId,
    pub normalized_value: String,
    pub provenance: TagProvenance,
    pub node_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TagAssociationEvidence {
    pub space_id: SpaceId,
    pub left: TagId,
    pub right: TagId,
    explicit_evidence: u32,
    generated_evidence: u32,
    weight: f32,
}

impl TagAssociationEvidence {
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

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct DocumentKey {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct MembershipKey {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    tag_id: TagId,
    provenance: TagProvenance,
    node_id: Option<String>,
}

/// A deterministic Space-local association graph over global Tag identities.
///
/// Membership rows retain their source provenance. Association weights are
/// derived from those rows and are never used to rewrite the dictionary or
/// Authority state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagGraph {
    memberships: BTreeMap<MembershipKey, TagMembershipEvidence>,
    edges: BTreeMap<(SpaceId, TagId, TagId), TagAssociationEvidence>,
}

impl TagGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an idempotent membership and rebuild the affected static graph.
    pub fn insert_membership(
        &mut self,
        dictionary: &mut TagDictionary,
        input: TagMembershipInput,
    ) -> Result<TagId, DerivedError> {
        let tag_id = dictionary.intern(&input.value)?;
        let normalized_value = dictionary
            .value(tag_id)
            .ok_or_else(|| DerivedError::InvalidProjectionValue {
                value: "Tag dictionary lost the inserted identity".to_owned(),
            })?
            .to_owned();
        let key = MembershipKey {
            space_id: input.space_id,
            memory_id: input.memory_id,
            revision_id: input.revision_id,
            tag_id,
            provenance: input.provenance,
            node_id: input.node_id.clone(),
        };
        self.memberships
            .entry(key)
            .or_insert_with(|| TagMembershipEvidence {
                space_id: input.space_id,
                memory_id: input.memory_id,
                revision_id: input.revision_id,
                tag_id,
                normalized_value,
                provenance: input.provenance,
                node_id: input.node_id,
            });
        self.rebuild_edges();
        Ok(tag_id)
    }

    #[must_use]
    pub fn for_space(&self, space_id: SpaceId) -> TagSpaceGraph<'_> {
        TagSpaceGraph {
            graph: self,
            space_id,
        }
    }

    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-tag-graph-v1\0");
        for evidence in self.memberships.values() {
            hash_membership(&mut hasher, evidence);
        }
        for edge in self.edges.values() {
            hash_edge(&mut hasher, edge);
        }
        hasher.finalize().into()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.memberships.is_empty()
    }

    pub fn membership_evidence(&self) -> impl Iterator<Item = &TagMembershipEvidence> {
        self.memberships.values()
    }

    fn rebuild_edges(&mut self) {
        self.edges.clear();
        let mut documents =
            BTreeMap::<DocumentKey, BTreeMap<TagId, BTreeSet<TagProvenance>>>::new();
        for evidence in self.memberships.values() {
            documents
                .entry(DocumentKey {
                    space_id: evidence.space_id,
                    memory_id: evidence.memory_id,
                    revision_id: evidence.revision_id,
                })
                .or_default()
                .entry(evidence.tag_id)
                .or_default()
                .insert(evidence.provenance);
        }

        for (document, tags) in documents {
            let tag_ids = tags.keys().copied().collect::<Vec<_>>();
            for (index, left) in tag_ids.iter().enumerate() {
                for right in tag_ids.iter().skip(index + 1) {
                    let left_provenance = &tags[left];
                    let right_provenance = &tags[right];
                    let explicit_evidence = u32::from(
                        left_provenance.contains(&TagProvenance::Explicit)
                            && right_provenance.contains(&TagProvenance::Explicit),
                    );
                    let generated_evidence = u32::from(
                        left_provenance.contains(&TagProvenance::Generated)
                            && right_provenance.contains(&TagProvenance::Generated),
                    );
                    if explicit_evidence == 0 && generated_evidence == 0 {
                        continue;
                    }
                    let key = (document.space_id, *left, *right);
                    let entry = self.edges.entry(key).or_insert(TagAssociationEvidence {
                        space_id: document.space_id,
                        left: *left,
                        right: *right,
                        explicit_evidence: 0,
                        generated_evidence: 0,
                        weight: 0.0,
                    });
                    entry.explicit_evidence += explicit_evidence;
                    entry.generated_evidence += generated_evidence;
                    entry.weight =
                        entry.explicit_evidence as f32 + entry.generated_evidence as f32 * 0.5;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TagSpaceGraph<'a> {
    graph: &'a TagGraph,
    space_id: SpaceId,
}

impl<'a> TagSpaceGraph<'a> {
    #[must_use]
    pub const fn space_id(self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub fn membership_count(self, tag_id: TagId) -> usize {
        self.graph
            .memberships
            .values()
            .filter(|evidence| evidence.space_id == self.space_id && evidence.tag_id == tag_id)
            .map(|evidence| (evidence.memory_id, evidence.revision_id))
            .collect::<BTreeSet<_>>()
            .len()
    }

    pub fn memberships(self) -> impl Iterator<Item = &'a TagMembershipEvidence> {
        self.graph
            .memberships
            .values()
            .filter(move |evidence| evidence.space_id == self.space_id)
    }

    #[must_use]
    pub fn tag_ids(self) -> Vec<TagId> {
        self.memberships()
            .map(|evidence| evidence.tag_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[must_use]
    pub fn edge(self, left: TagId, right: TagId) -> Option<&'a TagAssociationEvidence> {
        let (left, right) = ordered_pair(left, right);
        self.graph.edges.get(&(self.space_id, left, right))
    }

    pub fn edges(self) -> impl Iterator<Item = &'a TagAssociationEvidence> {
        self.graph
            .edges
            .values()
            .filter(move |edge| edge.space_id == self.space_id)
    }

    #[must_use]
    pub fn fingerprint(self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-tag-space-graph-v1\0");
        hasher.update(self.space_id.as_bytes());
        for evidence in self.memberships() {
            hash_membership(&mut hasher, evidence);
        }
        for edge in self.edges() {
            hash_edge(&mut hasher, edge);
        }
        hasher.finalize().into()
    }
}

fn ordered_pair(left: TagId, right: TagId) -> (TagId, TagId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn hash_membership(hasher: &mut Sha256, evidence: &TagMembershipEvidence) {
    hasher.update(evidence.space_id.as_bytes());
    hasher.update(evidence.memory_id.as_bytes());
    hasher.update(evidence.revision_id.as_bytes());
    hasher.update(evidence.tag_id.as_bytes());
    hash_string(hasher, &evidence.normalized_value);
    hasher.update([match evidence.provenance {
        TagProvenance::Explicit => 0,
        TagProvenance::Generated => 1,
    }]);
    if let Some(node_id) = &evidence.node_id {
        hasher.update([1]);
        hash_string(hasher, node_id);
    } else {
        hasher.update([0]);
    }
}

fn hash_edge(hasher: &mut Sha256, edge: &TagAssociationEvidence) {
    hasher.update(edge.space_id.as_bytes());
    hasher.update(edge.left.as_bytes());
    hasher.update(edge.right.as_bytes());
    hasher.update(edge.explicit_evidence.to_be_bytes());
    hasher.update(edge.generated_evidence.to_be_bytes());
    hasher.update(edge.weight.to_bits().to_be_bytes());
}

fn hash_string(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}
