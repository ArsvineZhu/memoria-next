pub mod embedding_view;
pub mod entities;
pub mod lexical;
pub mod relations;
pub mod rerank_view;
pub mod structural;
pub mod tags;
pub mod temporal;

use memoria_types::{MemoryId, RevisionId, SpaceId};

pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionTarget {
    pub space_id: Option<SpaceId>,
    pub memory_id: Option<MemoryId>,
    pub revision_id: Option<RevisionId>,
}
