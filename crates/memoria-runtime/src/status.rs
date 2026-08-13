use memoria_types::AuthorityGeneration;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeStatus {
    pub authority_generation: AuthorityGeneration,
    pub base_coverage: AuthorityGeneration,
    pub semantic_coverage: AuthorityGeneration,
    pub active_read_leases: usize,
    pub closed: bool,
    pub last_error: Option<String>,
}
