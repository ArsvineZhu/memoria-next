use memoria_types::SpaceId;

use crate::LexicalCandidate;
use crate::validate::QueryError;

pub trait LexicalOperator {
    fn search(
        &self,
        query: &str,
        scope: &[SpaceId],
        limit: usize,
    ) -> Result<Vec<LexicalCandidate>, QueryError>;
}
