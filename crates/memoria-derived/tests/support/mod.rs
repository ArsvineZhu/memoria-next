#![allow(dead_code)]

use memoria_derived::{
    LexicalDocument, TagDictionary, TagGraph, TagId, TagMembershipInput, TagProvenance,
    TagSpaceGraph,
};
use memoria_mdx::compile_ir;
use memoria_types::{MemoryId, RevisionId, SpaceId};

pub struct TagFixture {
    pub dictionary: TagDictionary,
    pub tag_graph: TagGraph,
}

pub struct AddedTag {
    pub tag_id: TagId,
}

impl TagFixture {
    pub fn new() -> Self {
        Self {
            dictionary: TagDictionary::new(),
            tag_graph: TagGraph::new(),
        }
    }

    pub fn add_tag(&mut self, space: u8, value: &str, memory: u8) -> AddedTag {
        self.add_tag_with_provenance(space, value, memory, TagProvenance::Explicit)
    }

    pub fn add_tag_with_provenance(
        &mut self,
        space: u8,
        value: &str,
        memory: u8,
        provenance: TagProvenance,
    ) -> AddedTag {
        let space_id = SpaceId::from_bytes([space; 16]);
        let memory_id = MemoryId::from_bytes([memory; 16]);
        let revision_id = RevisionId::from_bytes([memory; 32]);
        let tag_id = self
            .tag_graph
            .insert_membership(
                &mut self.dictionary,
                TagMembershipInput::new(space_id, memory_id, revision_id, value, provenance),
            )
            .unwrap();
        AddedTag { tag_id }
    }

    pub fn graph(&self, space: u8) -> TagSpaceGraph<'_> {
        self.tag_graph.for_space(SpaceId::from_bytes([space; 16]))
    }
}

pub fn enrichment_document(source: &str) -> LexicalDocument {
    LexicalDocument::new(
        SpaceId::from_bytes([9; 16]),
        MemoryId::from_bytes([8; 16]),
        RevisionId::from_bytes([7; 32]),
        compile_ir(source).unwrap(),
    )
}
