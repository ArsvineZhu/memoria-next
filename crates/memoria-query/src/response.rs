use crate::assessment::{RecallAssessment, assess};
use crate::compile::CompiledQuery;
use crate::consolidate::{ConsolidationCandidate, ConsolidationError, MemoryResult, consolidate};

#[derive(Clone, Debug, PartialEq)]
pub struct RetrievalResponse {
    pub retrieval_id: String,
    pub snapshot: crate::QuerySnapshot,
    pub execution: crate::CapabilityExecution,
    pub trace: crate::QueryOperatorTrace,
    pub results: Vec<MemoryResult>,
    pub assessment: RecallAssessment,
}

#[must_use]
pub fn result_id_for(retrieval_id: &str, index: usize) -> String {
    format!("{retrieval_id}:result:{index}")
}

pub fn build_response<I, C>(
    retrieval_id: impl Into<String>,
    compiled: &CompiledQuery,
    candidates: I,
) -> Result<RetrievalResponse, ConsolidationError>
where
    I: IntoIterator<Item = C>,
    C: Into<ConsolidationCandidate>,
{
    let retrieval_id = retrieval_id.into();
    let results = consolidate(candidates, compiled.query.budget)?;
    let assessment = assess(&results);
    Ok(RetrievalResponse {
        retrieval_id,
        snapshot: compiled.snapshot.clone(),
        execution: compiled.execution.clone(),
        trace: crate::QueryOperatorTrace {
            authority_generation: compiled.snapshot.authority_generation,
            ..crate::QueryOperatorTrace::default()
        },
        results,
        assessment,
    })
}
