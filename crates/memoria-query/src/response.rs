use crate::assessment::{RecallAssessment, assess};
use crate::compile::CompiledQuery;
use crate::consolidate::{ConsolidationCandidate, ConsolidationError, MemoryResult, consolidate};

#[derive(Clone, Debug, PartialEq)]
pub struct RetrievalResponse {
    pub retrieval_id: String,
    pub snapshot: crate::QuerySnapshot,
    pub execution: crate::CapabilityExecution,
    pub results: Vec<MemoryResult>,
    pub assessment: RecallAssessment,
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
    let mut results = consolidate(candidates, compiled.query.budget)?;
    for (index, result) in results.iter_mut().enumerate() {
        result.result_id = format!("{retrieval_id}:result:{index}");
    }
    let assessment = assess(&results);
    Ok(RetrievalResponse {
        retrieval_id,
        snapshot: compiled.snapshot.clone(),
        execution: compiled.execution.clone(),
        results,
        assessment,
    })
}
