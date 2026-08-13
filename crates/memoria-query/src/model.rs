use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use memoria_mdx::TemporalValue;
use memoria_types::{AuthorityGeneration, MemoryId, SpaceId};

use crate::validate::QueryError;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityRef(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityRefParseError {
    value: String,
}

impl fmt::Display for EntityRefParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid entity reference `{}`", self.value)
    }
}

impl std::error::Error for EntityRefParseError {}

impl EntityRef {
    pub fn new(value: impl Into<String>) -> Result<Self, QueryError> {
        let value = value.into();
        validate_entity_ref(&value).map_err(|_| QueryError::InvalidEntityRef {
            value: value.clone(),
        })?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for EntityRef {
    type Err = EntityRefParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        validate_entity_ref(value).map_err(|_| EntityRefParseError {
            value: value.to_owned(),
        })?;
        Ok(Self(value.to_owned()))
    }
}

fn validate_entity_ref(value: &str) -> Result<(), ()> {
    if value.is_empty() || !value.contains(':') || value.chars().any(char::is_whitespace) {
        return Err(());
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryScope {
    pub spaces: Vec<SpaceId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryCue {
    pub text: Vec<String>,
    pub tags: Vec<String>,
    pub entities: Vec<EntityRef>,
    pub memories: Vec<MemoryId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryConstraints {
    pub entities: Vec<EntityRef>,
    pub memories: Vec<MemoryId>,
    pub tags: Vec<String>,
    pub valid_at: Option<TemporalValue>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AuthorityConsistency {
    #[default]
    Latest,
    AtLeast(AuthorityGeneration),
    Pinned(AuthorityGeneration),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ReadinessBehavior {
    Wait(Duration),
    #[default]
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryConsistency {
    pub authority: AuthorityConsistency,
    pub readiness: ReadinessBehavior,
}

impl Default for QueryConsistency {
    fn default() -> Self {
        Self {
            authority: AuthorityConsistency::Latest,
            readiness: ReadinessBehavior::Fail,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryBudget {
    pub max_results: usize,
    pub max_candidates: usize,
    pub timeout: Duration,
}

impl QueryBudget {
    #[must_use]
    pub const fn new(max_results: usize, max_candidates: usize, timeout: Duration) -> Self {
        Self {
            max_results,
            max_candidates,
            timeout,
        }
    }
}

impl Default for QueryBudget {
    fn default() -> Self {
        Self::new(20, 200, Duration::from_millis(1_500))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct QueryQuality {
    pub min_relevance: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryQuery {
    pub scope: QueryScope,
    pub cue: QueryCue,
    pub constraints: QueryConstraints,
    pub consistency: QueryConsistency,
    pub required_capabilities: Vec<String>,
    pub preferred_capabilities: Vec<String>,
    pub budget: QueryBudget,
    pub quality: QueryQuality,
}

impl MemoryQuery {
    #[must_use]
    pub fn builder() -> MemoryQueryBuilder {
        MemoryQueryBuilder::default()
    }

    pub fn validate(&self) -> Result<(), QueryError> {
        crate::validate::validate_query(self)
    }
}

#[derive(Clone, Debug, Default)]
pub struct MemoryQueryBuilder {
    scope: QueryScope,
    cue: QueryCue,
    constraints: QueryConstraints,
    consistency: QueryConsistency,
    required_capabilities: Vec<String>,
    preferred_capabilities: Vec<String>,
    budget: QueryBudget,
    quality: QueryQuality,
}

impl MemoryQueryBuilder {
    #[must_use]
    pub fn spaces(mut self, spaces: Vec<SpaceId>) -> Self {
        self.scope.spaces = spaces;
        self
    }

    #[must_use]
    pub fn text_cue(mut self, text: impl Into<String>) -> Self {
        self.cue.text.push(text.into());
        self
    }

    #[must_use]
    pub fn cue_tag(mut self, tag: impl Into<String>) -> Self {
        self.cue.tags.push(tag.into());
        self
    }

    #[must_use]
    pub fn cue_entity(mut self, entity: EntityRef) -> Self {
        self.cue.entities.push(entity);
        self
    }

    #[must_use]
    pub fn cue_memory(mut self, memory: MemoryId) -> Self {
        self.cue.memories.push(memory);
        self
    }

    #[must_use]
    pub fn require_entity(mut self, entity: EntityRef) -> Self {
        self.constraints.entities.push(entity);
        self
    }

    #[must_use]
    pub fn require_memory(mut self, memory: MemoryId) -> Self {
        self.constraints.memories.push(memory);
        self
    }

    #[must_use]
    pub fn require_tag(mut self, tag: impl Into<String>) -> Self {
        self.constraints.tags.push(tag.into());
        self
    }

    #[must_use]
    pub fn valid_at(mut self, value: TemporalValue) -> Self {
        self.constraints.valid_at = Some(value);
        self
    }

    #[must_use]
    pub fn require_capability(mut self, capability: impl Into<String>) -> Self {
        self.required_capabilities.push(capability.into());
        self
    }

    #[must_use]
    pub fn prefer_capability(mut self, capability: impl Into<String>) -> Self {
        self.preferred_capabilities.push(capability.into());
        self
    }

    #[must_use]
    pub fn authority_at_least(mut self, generation: AuthorityGeneration) -> Self {
        self.consistency.authority = AuthorityConsistency::AtLeast(generation);
        self
    }

    #[must_use]
    pub fn pin_authority(mut self, generation: AuthorityGeneration) -> Self {
        self.consistency.authority = AuthorityConsistency::Pinned(generation);
        self
    }

    #[must_use]
    pub fn fail_if_not_ready(mut self) -> Self {
        self.consistency.readiness = ReadinessBehavior::Fail;
        self
    }

    #[must_use]
    pub fn wait_for(mut self, timeout: Duration) -> Self {
        self.consistency.readiness = ReadinessBehavior::Wait(timeout);
        self
    }

    #[must_use]
    pub fn budget(mut self, budget: QueryBudget) -> Self {
        self.budget = budget;
        self
    }

    #[must_use]
    pub fn max_results(mut self, max_results: usize) -> Self {
        self.budget.max_results = max_results;
        self
    }

    #[must_use]
    pub fn min_relevance(mut self, min_relevance: f32) -> Self {
        self.quality.min_relevance = Some(min_relevance);
        self
    }

    pub fn build(self) -> Result<MemoryQuery, QueryError> {
        let query = self.build_unchecked();
        query.validate()?;
        Ok(query)
    }

    #[must_use]
    pub fn build_unchecked(self) -> MemoryQuery {
        MemoryQuery {
            scope: self.scope,
            cue: self.cue,
            constraints: self.constraints,
            consistency: self.consistency,
            required_capabilities: self.required_capabilities,
            preferred_capabilities: self.preferred_capabilities,
            budget: self.budget,
            quality: self.quality,
        }
    }
}
