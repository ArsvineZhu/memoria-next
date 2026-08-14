use std::collections::BTreeSet;

use thiserror::Error;

use memoria_types::AuthorityGeneration;

use crate::model::MemoryQuery;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum QueryError {
    #[error("query scope must name at least one Space")]
    ScopeRequired,

    #[error("query scope contains duplicate Space IDs")]
    DuplicateScope,

    #[error("invalid entity reference `{value}`")]
    InvalidEntityRef { value: String },

    #[error("text cue cannot be empty")]
    EmptyTextCue,

    #[error("tag cue or constraint cannot be empty")]
    EmptyTag,

    #[error("invalid Tag cue: {value}")]
    InvalidTagCue { value: String },

    #[error("capability name cannot be empty")]
    EmptyCapability,

    #[error("required and preferred capability sets overlap")]
    CapabilityOverlap,

    #[error("query result and candidate budgets must be greater than zero")]
    InvalidBudget,

    #[error("minimum relevance must be finite and between 0 and 1")]
    InvalidQuality,

    #[error("invalid query value for {field}: {value}")]
    InvalidQueryValue { field: String, value: String },

    #[error("Authority generation {actual} is below required generation {required}")]
    AuthorityNotReady {
        required: AuthorityGeneration,
        actual: AuthorityGeneration,
    },

    #[error(
        "required capability `{capability}` is not ready at {required}; available coverage is {available}"
    )]
    CapabilityNotReady {
        capability: String,
        required: AuthorityGeneration,
        available: AuthorityGeneration,
    },

    #[error("a Derived Manifest is required to compile this query")]
    DerivedSnapshotUnavailable,
}

pub(crate) fn validate_query(query: &MemoryQuery) -> Result<(), QueryError> {
    if query.scope.spaces.is_empty() {
        return Err(QueryError::ScopeRequired);
    }
    let unique_spaces = query.scope.spaces.iter().collect::<BTreeSet<_>>();
    if unique_spaces.len() != query.scope.spaces.len() {
        return Err(QueryError::DuplicateScope);
    }
    if query.cue.text.iter().any(|text| text.trim().is_empty()) {
        return Err(QueryError::EmptyTextCue);
    }
    if query
        .cue
        .tags
        .iter()
        .chain(query.constraints.tags.iter())
        .any(|tag| tag.trim().is_empty())
    {
        return Err(QueryError::EmptyTag);
    }
    if query
        .required_capabilities
        .iter()
        .chain(query.preferred_capabilities.iter())
        .any(|capability| capability.trim().is_empty())
    {
        return Err(QueryError::EmptyCapability);
    }
    let required = query
        .required_capabilities
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if query
        .preferred_capabilities
        .iter()
        .any(|capability| required.contains(capability.as_str()))
    {
        return Err(QueryError::CapabilityOverlap);
    }
    if query.budget.max_results == 0
        || query.budget.max_candidates == 0
        || query.budget.max_matches_per_result == 0
        || query.budget.max_evidence_tokens == 0
    {
        return Err(QueryError::InvalidBudget);
    }
    if query
        .quality
        .min_relevance
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(QueryError::InvalidQuality);
    }
    Ok(())
}
