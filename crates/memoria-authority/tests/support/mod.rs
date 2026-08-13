use memoria_authority::{AuthorityDb, SourceCas};
use memoria_types::{MemoriaError, MemoryId, RevisionId, RevisionSemanticIntent};

pub fn test_only_create_detached_revision(
    db: &AuthorityDb,
    cas: &SourceCas,
    memory_id: MemoryId,
    parents: Vec<RevisionId>,
    source: &[u8],
    semantic_intent: RevisionSemanticIntent,
) -> Result<RevisionId, MemoriaError> {
    db.test_only_create_detached_revision(cas, memory_id, parents, source, semantic_intent)
        .map(|result| result.into_value())
}
