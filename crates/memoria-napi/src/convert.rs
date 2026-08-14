use std::time::Duration;

use memoria_authority::{SpaceProviderMode, SpaceProviderPolicy};
use memoria_mdx::TemporalValue;
use memoria_query::{
    AuthorityConsistency, EntityRef, MemoryQuery, MemoryReference, QueryBudget, QueryConsistency,
    QueryConstraints, QueryCue, QueryError, QueryHistory, QueryHistoryMode, QueryLifecycle,
    QueryQualityLevel, ReadinessBehavior,
};
use memoria_runtime::{
    BackupManifest, EmbeddingVector, FeedbackCommit, FeedbackSubmission, FeedbackSubmissionEvent,
    NeedWork, PortableImportRequest, PortableImportResult, PortableMemory, ProviderCapability,
    ProviderRoute, ProviderRouteConfig, ProviderTrust, ProviderWorkResult, PurgePlan, PurgeState,
    QueryStep, QueryWork, RerankScore,
};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

use crate::error::to_napi_error;

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryMemoryReference {
    pub memory_id: String,
    pub revision_id: Option<String>,
    pub node_id: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryCue {
    pub text: Option<String>,
    pub tags: Option<Vec<String>>,
    pub entities: Option<Vec<String>>,
    pub memories: Option<Vec<JsQueryMemoryReference>>,
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryConstraints {
    pub tags: Option<Vec<String>>,
    pub entities: Option<Vec<String>>,
    pub memories: Option<Vec<JsQueryMemoryReference>>,
    pub lifecycle: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryTemporal {
    pub valid_at: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryHistory {
    pub mode: String,
    pub from_authority_generation: Option<String>,
    pub to_authority_generation: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JsQueryAuthority {
    pub mode: String,
    pub generation: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsQueryConsistency {
    pub authority: JsQueryAuthority,
    pub required: Vec<String>,
    pub preferred: Vec<String>,
    pub on_not_ready: String,
    pub timeout_ms: u32,
}

impl Default for JsQueryConsistency {
    fn default() -> Self {
        Self {
            authority: JsQueryAuthority {
                mode: "latest".to_owned(),
                generation: None,
            },
            required: Vec::new(),
            preferred: Vec::new(),
            on_not_ready: "fail".to_owned(),
            timeout_ms: 5_000,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsQueryBudget {
    pub max_results: u32,
    pub max_matches_per_result: u32,
    pub max_evidence_tokens: u32,
}

impl Default for JsQueryBudget {
    fn default() -> Self {
        Self {
            max_results: 10,
            max_matches_per_result: 3,
            max_evidence_tokens: 1_500,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsQueryRequest {
    pub scope: Vec<String>,
    pub cue: Option<JsQueryCue>,
    pub constraints: Option<JsQueryConstraints>,
    pub temporal: Option<JsQueryTemporal>,
    pub history: Option<JsQueryHistory>,
    pub consistency: JsQueryConsistency,
    pub budget: JsQueryBudget,
    pub quality: String,
}

impl JsQueryRequest {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            scope: vec![SpaceId::from_bytes([1; 16]).to_string()],
            cue: Some(JsQueryCue {
                text: Some("career".to_owned()),
                ..JsQueryCue::default()
            }),
            constraints: None,
            temporal: None,
            history: Some(JsQueryHistory {
                mode: "current".to_owned(),
                ..JsQueryHistory::default()
            }),
            consistency: JsQueryConsistency::default(),
            budget: JsQueryBudget::default(),
            quality: "balanced".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    pub scope: Vec<SpaceId>,
    pub cue: Option<JsQueryCue>,
    pub constraints: Option<JsQueryConstraints>,
    pub temporal: Option<JsQueryTemporal>,
    pub history: Option<JsQueryHistory>,
    pub consistency: JsQueryConsistency,
    pub budget: JsQueryBudget,
    pub quality: String,
}

impl QueryRequest {
    pub fn into_core(self) -> Result<MemoryQuery> {
        let mut builder = MemoryQuery::builder().spaces(self.scope);
        if let Some(cue) = self.cue {
            builder = builder.cue(parse_cue(cue)?);
        }
        let mut constraints = self
            .constraints
            .map(parse_constraints)
            .transpose()?
            .unwrap_or_default();
        if let Some(temporal) = self.temporal
            && let Some(valid_at) = temporal.valid_at
        {
            constraints.valid_at = Some(
                valid_at
                    .parse::<TemporalValue>()
                    .map_err(|error| invalid_query("temporal.validAt", error.to_string()))?,
            );
        }
        builder = builder.constraints(constraints);
        builder = builder.history(parse_history(self.history.unwrap_or_else(|| {
            JsQueryHistory {
                mode: "current".to_owned(),
                ..JsQueryHistory::default()
            }
        }))?);
        let consistency = self.consistency;
        for capability in &consistency.required {
            builder = builder.require_capability(capability.clone());
        }
        for capability in &consistency.preferred {
            builder = builder.prefer_capability(capability.clone());
        }
        builder = builder.consistency(parse_consistency(consistency)?);
        builder = builder
            .budget(parse_budget(self.budget)?)
            .quality(parse_quality(&self.quality)?);
        builder.build().map_err(query_error)
    }
}

impl TryFrom<JsQueryRequest> for QueryRequest {
    type Error = napi::Error;

    fn try_from(value: JsQueryRequest) -> Result<Self> {
        let scope = value
            .scope
            .into_iter()
            .map(|space| {
                space
                    .parse::<SpaceId>()
                    .map_err(|error| invalid_query("scope.spaces", error.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            scope,
            cue: value.cue,
            constraints: value.constraints,
            temporal: value.temporal,
            history: value.history,
            consistency: value.consistency,
            budget: value.budget,
            quality: value.quality,
        })
    }
}

impl From<QueryRequest> for JsQueryRequest {
    fn from(value: QueryRequest) -> Self {
        Self {
            scope: value
                .scope
                .into_iter()
                .map(|space| space.to_string())
                .collect(),
            cue: value.cue,
            constraints: value.constraints,
            temporal: value.temporal,
            history: value.history,
            consistency: value.consistency,
            budget: value.budget,
            quality: value.quality,
        }
    }
}

fn query_error(error: QueryError) -> napi::Error {
    napi::Error::from_reason(format!("QUERY_ERROR: {error}"))
}

fn invalid_query(field: &str, value: impl Into<String>) -> napi::Error {
    query_error(QueryError::InvalidQueryValue {
        field: field.to_owned(),
        value: value.into(),
    })
}

fn parse_cue(value: JsQueryCue) -> Result<QueryCue> {
    let entities = value
        .entities
        .unwrap_or_default()
        .into_iter()
        .map(|value| {
            EntityRef::new(value).map_err(|error| {
                query_error(QueryError::InvalidEntityRef {
                    value: error.value().to_owned(),
                })
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let memories = value
        .memories
        .unwrap_or_default()
        .into_iter()
        .map(parse_memory_reference)
        .collect::<Result<Vec<_>>>()?;
    Ok(QueryCue {
        text: value.text.into_iter().collect(),
        tags: value.tags.unwrap_or_default(),
        entities,
        memories,
    })
}

fn parse_constraints(value: JsQueryConstraints) -> Result<QueryConstraints> {
    let entities = value
        .entities
        .unwrap_or_default()
        .into_iter()
        .map(|value| {
            EntityRef::new(value).map_err(|error| {
                query_error(QueryError::InvalidEntityRef {
                    value: error.value().to_owned(),
                })
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let memories = value
        .memories
        .unwrap_or_default()
        .into_iter()
        .map(parse_memory_reference)
        .collect::<Result<Vec<_>>>()?;
    let lifecycle = value
        .lifecycle
        .as_deref()
        .map(parse_lifecycle)
        .transpose()?;
    Ok(QueryConstraints {
        entities,
        memories,
        tags: value.tags.unwrap_or_default(),
        valid_at: None,
        lifecycle,
    })
}

fn parse_memory_reference(value: JsQueryMemoryReference) -> Result<MemoryReference> {
    let memory_id = value
        .memory_id
        .parse::<MemoryId>()
        .map_err(|error| invalid_query("memory.memoryId", error.to_string()))?;
    let revision_id = value
        .revision_id
        .map(|revision| {
            revision
                .parse::<RevisionId>()
                .map_err(|error| invalid_query("memory.revisionId", error.to_string()))
        })
        .transpose()?;
    Ok(MemoryReference {
        memory_id,
        revision_id,
        node_id: value.node_id,
    })
}

fn parse_history(value: JsQueryHistory) -> Result<QueryHistory> {
    Ok(QueryHistory {
        mode: match value.mode.as_str() {
            "current" => QueryHistoryMode::Current,
            "all-revisions" => QueryHistoryMode::AllRevisions,
            "changes" => QueryHistoryMode::Changes,
            other => return Err(invalid_query("history.mode", other)),
        },
        from_authority_generation: parse_generation(
            "history.fromAuthorityGeneration",
            value.from_authority_generation,
        )?,
        to_authority_generation: parse_generation(
            "history.toAuthorityGeneration",
            value.to_authority_generation,
        )?,
    })
}

fn parse_generation(
    field: &str,
    value: Option<String>,
) -> Result<Option<memoria_types::AuthorityGeneration>> {
    value
        .map(|value| {
            value
                .parse::<AuthorityGeneration>()
                .map_err(|error| invalid_query(field, error.to_string()))
        })
        .transpose()
}

fn parse_consistency(value: JsQueryConsistency) -> Result<QueryConsistency> {
    let authority = match (value.authority.mode.as_str(), value.authority.generation) {
        ("latest", None) => AuthorityConsistency::Latest,
        ("at-least", Some(generation)) => {
            AuthorityConsistency::AtLeast(generation.parse::<AuthorityGeneration>().map_err(
                |error| invalid_query("consistency.authority.generation", error.to_string()),
            )?)
        }
        ("exact", Some(generation)) => {
            AuthorityConsistency::Pinned(generation.parse::<AuthorityGeneration>().map_err(
                |error| invalid_query("consistency.authority.generation", error.to_string()),
            )?)
        }
        (mode, generation) => {
            return Err(invalid_query(
                "consistency.authority",
                format!("mode={mode}, generation={generation:?}"),
            ));
        }
    };
    let readiness = match value.on_not_ready.as_str() {
        "fail" => ReadinessBehavior::Fail,
        "wait" => ReadinessBehavior::Wait(Duration::from_millis(u64::from(value.timeout_ms))),
        other => return Err(invalid_query("consistency.onNotReady", other)),
    };
    Ok(QueryConsistency {
        authority,
        readiness,
        timeout: Duration::from_millis(u64::from(value.timeout_ms)),
    })
}

fn parse_budget(value: JsQueryBudget) -> Result<QueryBudget> {
    let max_results = usize::try_from(value.max_results)
        .map_err(|error| invalid_query("budget.maxResults", error.to_string()))?;
    let max_matches_per_result = usize::try_from(value.max_matches_per_result)
        .map_err(|error| invalid_query("budget.maxMatchesPerResult", error.to_string()))?;
    let max_evidence_tokens = usize::try_from(value.max_evidence_tokens)
        .map_err(|error| invalid_query("budget.maxEvidenceTokens", error.to_string()))?;
    Ok(QueryBudget::default().with_normalized_limits(
        max_results,
        max_matches_per_result,
        max_evidence_tokens,
    ))
}

fn parse_lifecycle(value: &str) -> Result<QueryLifecycle> {
    match value {
        "active" => Ok(QueryLifecycle::Active),
        "retired" => Ok(QueryLifecycle::Retired),
        "any" => Ok(QueryLifecycle::Any),
        other => Err(invalid_query("constraints.lifecycle", other)),
    }
}

fn parse_quality(value: &str) -> Result<QueryQualityLevel> {
    match value {
        "fast" => Ok(QueryQualityLevel::Fast),
        "balanced" => Ok(QueryQualityLevel::Balanced),
        "thorough" => Ok(QueryQualityLevel::Thorough),
        other => Err(invalid_query("quality", other)),
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsCreateMemoryRequest {
    pub space_id: String,
    pub document_key: Option<String>,
    pub idempotency_key: Option<String>,
    pub mdx: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsReviseMemoryRequest {
    pub memory_id: String,
    pub expected_head: String,
    pub mdx: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsMemoryMutation {
    pub memory_id: String,
    pub space_id: String,
    pub revision_id: String,
    pub authority_generation: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsStatus {
    pub authority_generation: String,
    pub base_coverage: String,
    pub semantic_coverage: String,
    pub semantic_build_coverage: String,
    pub active_read_leases: u32,
    pub closed: bool,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryResponse {
    pub result_count: u32,
    pub authority_generation: String,
    pub degraded: bool,
    pub retrieval_id: String,
    pub trace: JsQueryTrace,
    pub results: Vec<JsQueryResult>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryTrace {
    pub channels_executed: Vec<String>,
    pub candidate_counts: Vec<JsQueryChannelCount>,
    pub tag_basis_rank: Option<u32>,
    pub tag_basis_conditioning: Option<f64>,
    pub tag_basis_explained_energy: Option<f64>,
    pub activation_edge_visits: u32,
    pub activation_hops: u32,
    pub activation_truncated: bool,
    pub diffusion_iterations: u32,
    pub diffusion_convergence_delta: Option<f64>,
    pub diffusion_truncated: bool,
    pub independent_support_count: u32,
    pub correlation_suppressed_evidence: u32,
    pub relation_expansions: u32,
    pub rerank_requested: bool,
    pub rerank_applied: bool,
    pub capability_degraded: bool,
    pub authority_generation: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryChannelCount {
    pub channel: String,
    pub count: u32,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryStep {
    pub state: String,
    pub response: Option<JsQueryResponse>,
    pub operation_id: Option<String>,
    pub work: Option<JsQueryWork>,
    pub retry_after_ms: Option<u32>,
    pub deadline_unix_ms: Option<f64>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsSpaceProviderPolicy {
    pub embedding: String,
    pub reranking: String,
    pub enrichment: String,
}

impl From<SpaceProviderPolicy> for JsSpaceProviderPolicy {
    fn from(value: SpaceProviderPolicy) -> Self {
        Self {
            embedding: provider_mode_to_js(value.embedding).to_owned(),
            reranking: provider_mode_to_js(value.reranking).to_owned(),
            enrichment: provider_mode_to_js(value.enrichment).to_owned(),
        }
    }
}

impl TryFrom<JsSpaceProviderPolicy> for SpaceProviderPolicy {
    type Error = napi::Error;

    fn try_from(value: JsSpaceProviderPolicy) -> Result<Self> {
        Ok(Self {
            embedding: provider_mode_from_js("embedding", &value.embedding)?,
            reranking: provider_mode_from_js("reranking", &value.reranking)?,
            enrichment: provider_mode_from_js("enrichment", &value.enrichment)?,
        })
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderRoute {
    pub capability: String,
    pub trust: String,
    pub signature: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderRouteConfig {
    pub embedding: Option<JsProviderRoute>,
    pub rerank: Option<JsProviderRoute>,
    pub enrichment: Option<JsProviderRoute>,
}

pub fn provider_routes_from_js(
    value: Option<JsProviderRouteConfig>,
) -> Result<ProviderRouteConfig> {
    let mut routes = ProviderRouteConfig::default();
    let Some(value) = value else {
        return Ok(routes);
    };
    if let Some(route) = value.embedding {
        routes.embedding = provider_route_from_js(ProviderCapability::Embedding, route)?;
    }
    if let Some(route) = value.rerank {
        routes.rerank = provider_route_from_js(ProviderCapability::Rerank, route)?;
    }
    if let Some(route) = value.enrichment {
        routes.enrichment = provider_route_from_js(ProviderCapability::Enrichment, route)?;
    }
    Ok(routes)
}

fn provider_route_from_js(
    expected_capability: ProviderCapability,
    value: JsProviderRoute,
) -> Result<ProviderRoute> {
    let capability = match value.capability.as_str() {
        "embedding" => ProviderCapability::Embedding,
        "rerank" => ProviderCapability::Rerank,
        "enrichment" => ProviderCapability::Enrichment,
        other => {
            return Err(napi::Error::from_reason(format!(
                "UNSUPPORTED_OPERATION: provider route capability `{other}` is invalid"
            )));
        }
    };
    if capability != expected_capability {
        return Err(napi::Error::from_reason(format!(
            "UNSUPPORTED_OPERATION: provider route capability `{capability}` does not match the configured field"
        )));
    }
    let trust = match value.trust.as_str() {
        "local" => ProviderTrust::Local,
        "external" => ProviderTrust::External,
        other => {
            return Err(napi::Error::from_reason(format!(
                "UNSUPPORTED_OPERATION: provider route trust `{other}` is invalid"
            )));
        }
    };
    if value.signature.trim().is_empty() {
        return Err(napi::Error::from_reason(
            "UNSUPPORTED_OPERATION: provider route signature must not be empty",
        ));
    }
    Ok(ProviderRoute::new(capability, trust, value.signature))
}

impl From<ProviderRoute> for JsProviderRoute {
    fn from(value: ProviderRoute) -> Self {
        Self {
            capability: value.capability.to_string(),
            trust: match value.trust {
                ProviderTrust::Local => "local",
                ProviderTrust::External => "external",
            }
            .to_owned(),
            signature: value.signature,
        }
    }
}

fn provider_mode_to_js(value: SpaceProviderMode) -> &'static str {
    match value {
        SpaceProviderMode::Deny => "deny",
        SpaceProviderMode::LocalOnly => "local-only",
        SpaceProviderMode::ExternalAllowed => "external-allowed",
    }
}

fn provider_mode_from_js(field: &str, value: &str) -> Result<SpaceProviderMode> {
    match value {
        "deny" => Ok(SpaceProviderMode::Deny),
        "local-only" => Ok(SpaceProviderMode::LocalOnly),
        "external-allowed" => Ok(SpaceProviderMode::ExternalAllowed),
        _ => Err(napi::Error::from_reason(format!(
            "UNSUPPORTED_OPERATION: {field} provider policy mode `{value}` is invalid"
        ))),
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryWork {
    pub r#type: String,
    pub work_id: String,
    pub signature: String,
    pub route: JsProviderRoute,
    pub input: Option<JsProviderItem>,
    pub query: Option<String>,
    pub candidates: Vec<JsQueryCandidate>,
    pub space_policy: JsSpaceProviderPolicy,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryCandidate {
    pub handle: String,
    pub text: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryResult {
    pub result_id: String,
    pub space_id: String,
    pub memory_id: String,
    pub revision_id: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsFeedbackEvent {
    pub result_id: String,
    pub outcome: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsFeedbackSubmission {
    pub retrieval_id: String,
    pub idempotency_key: String,
    pub events: Vec<JsFeedbackEvent>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsFeedbackCommit {
    pub generation: String,
    pub events: Vec<JsFeedbackEventResult>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsFeedbackEventResult {
    pub event_id: String,
    pub generation: String,
    pub retrieval_id: String,
    pub space_id: String,
    pub memory_id: String,
    pub revision_id: String,
    pub semantic_node_id: Option<String>,
    pub outcome: String,
    pub occurred_at_seconds: i64,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsPortableMemory {
    pub source_id: String,
    pub space_id: String,
    pub revision_id: String,
    pub mdx: String,
}

impl From<PortableMemory> for JsPortableMemory {
    fn from(value: PortableMemory) -> Self {
        Self {
            source_id: value.source_id.to_string(),
            space_id: value.space_id.to_string(),
            revision_id: value.revision_id.to_string(),
            mdx: value.mdx,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsPortableImportRequest {
    pub target_space_key: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub origin_store_id: Option<String>,
    pub memories: Vec<JsPortableMemory>,
}

impl TryFrom<JsPortableImportRequest> for PortableImportRequest {
    type Error = napi::Error;

    fn try_from(value: JsPortableImportRequest) -> std::result::Result<Self, Self::Error> {
        let memories = value
            .memories
            .into_iter()
            .map(|memory| {
                Ok(PortableMemory {
                    source_id: memory.source_id.parse().map_err(to_napi_error)?,
                    space_id: memory.space_id.parse().map_err(to_napi_error)?,
                    revision_id: memory.revision_id.parse().map_err(to_napi_error)?,
                    mdx: memory.mdx,
                })
            })
            .collect::<std::result::Result<Vec<_>, napi::Error>>()?;
        Ok(PortableImportRequest {
            target_space_key: value.target_space_key,
            idempotency_key: value.idempotency_key,
            request_fingerprint: value.request_fingerprint,
            origin_store_id: value.origin_store_id,
            memories,
        })
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsPortableImportMapping {
    pub source_id: String,
    pub target_id: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsPortableImportResult {
    pub target_space_id: String,
    pub mappings: Vec<JsPortableImportMapping>,
    pub unresolved_external_references: Vec<String>,
}

impl From<PortableImportResult> for JsPortableImportResult {
    fn from(value: PortableImportResult) -> Self {
        Self {
            target_space_id: value.target_space_id.to_string(),
            mappings: value
                .mappings
                .into_iter()
                .map(|mapping| JsPortableImportMapping {
                    source_id: mapping.source_id.to_string(),
                    target_id: mapping.target_id.to_string(),
                })
                .collect(),
            unresolved_external_references: value.unresolved_external_references,
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsPurgePlan {
    pub id: String,
    pub memory_id: String,
    pub state: String,
}

#[napi(object)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsBackupResult {
    pub path: String,
    pub store_id: String,
    pub authority_generation: String,
    pub include_adaptive: bool,
    pub file_count: u32,
    pub source_object_count: u32,
    pub manifest_hash: String,
}

impl TryFrom<BackupManifest> for JsBackupResult {
    type Error = napi::Error;

    fn try_from(value: BackupManifest) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            path: value.path.to_string_lossy().into_owned(),
            store_id: value.store_id.to_string(),
            authority_generation: value.authority_generation.to_string(),
            include_adaptive: value.includes_adaptive,
            file_count: u32::try_from(value.file_count)
                .map_err(|error| napi::Error::from_reason(error.to_string()))?,
            source_object_count: u32::try_from(value.source_objects.len())
                .map_err(|error| napi::Error::from_reason(error.to_string()))?,
            manifest_hash: value.manifest_hash,
        })
    }
}

impl From<PurgePlan> for JsPurgePlan {
    fn from(value: PurgePlan) -> Self {
        Self {
            id: value.id,
            memory_id: value.memory_id.to_string(),
            state: match value.state {
                PurgeState::Planned => "planned",
                PurgeState::Committed => "committed",
                PurgeState::Cleaning => "cleaning",
                PurgeState::Completed => "completed",
            }
            .to_owned(),
        }
    }
}

impl TryFrom<JsFeedbackSubmission> for FeedbackSubmission {
    type Error = napi::Error;

    fn try_from(value: JsFeedbackSubmission) -> Result<Self> {
        let events = value
            .events
            .into_iter()
            .map(|event| {
                Ok(FeedbackSubmissionEvent {
                    result_id: event.result_id,
                    outcome: event.outcome.parse().map_err(
                        |error: memoria_adaptive::AdaptiveError| {
                            napi::Error::from_reason(error.to_string())
                        },
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(FeedbackSubmission {
            retrieval_id: value.retrieval_id,
            idempotency_key: value.idempotency_key,
            events,
        })
    }
}

impl From<FeedbackCommit> for JsFeedbackCommit {
    fn from(value: FeedbackCommit) -> Self {
        Self {
            generation: value.generation.to_string(),
            events: value
                .events
                .into_iter()
                .map(|event| JsFeedbackEventResult {
                    event_id: event.event_id,
                    generation: event.generation.to_string(),
                    retrieval_id: event.retrieval_id,
                    space_id: event.space_id.to_string(),
                    memory_id: event.memory_id.to_string(),
                    revision_id: event.revision_id.to_string(),
                    semantic_node_id: event.semantic_node_id,
                    outcome: event.outcome.to_string(),
                    occurred_at_seconds: event.occurred_at.unix_seconds(),
                })
                .collect(),
        }
    }
}

impl From<(String, memoria_query::MemoryResult)> for JsQueryResult {
    fn from((result_id, value): (String, memoria_query::MemoryResult)) -> Self {
        Self {
            result_id,
            space_id: value.space_id.to_string(),
            memory_id: value.memory_id.to_string(),
            revision_id: value.revision_id.to_string(),
        }
    }
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderResult {
    pub r#type: String,
    pub work_id: String,
    pub vectors: Option<Vec<JsEmbeddingVector>>,
    pub scores: Option<Vec<JsRerankScore>>,
    pub tags: Option<Vec<String>>,
    pub retryable: Option<bool>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsEmbeddingVector {
    pub key: String,
    pub values: Vec<f64>,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsRerankScore {
    pub handle: String,
    pub score: f64,
}

pub fn provider_result_from_js(result: JsProviderResult) -> Result<ProviderWorkResult> {
    let work_id = result.work_id;
    match result.r#type.as_str() {
        "embeddings" => {
            let vectors = result.vectors.ok_or_else(|| {
                napi::Error::from_reason(
                    "PROVIDER_UNAVAILABLE: embeddings result is missing vectors",
                )
            })?;
            Ok(ProviderWorkResult::Embeddings {
                work_id,
                vectors: vectors
                    .into_iter()
                    .map(|vector| {
                        Ok(EmbeddingVector {
                            key: vector.key,
                            values: vector
                                .values
                                .into_iter()
                                .map(provider_f32)
                                .collect::<Result<Vec<_>>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        "rerank" => {
            let scores = result.scores.ok_or_else(|| {
                napi::Error::from_reason("PROVIDER_UNAVAILABLE: rerank result is missing scores")
            })?;
            Ok(ProviderWorkResult::Rerank {
                work_id,
                scores: scores
                    .into_iter()
                    .map(|score| {
                        Ok(RerankScore {
                            handle: score.handle,
                            score: provider_f32(score.score)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        "enrichment" => {
            let tags = result.tags.ok_or_else(|| {
                napi::Error::from_reason("PROVIDER_UNAVAILABLE: enrichment result is missing tags")
            })?;
            Ok(ProviderWorkResult::Enrichment { work_id, tags })
        }
        "failure" => Ok(ProviderWorkResult::Failure {
            work_id,
            retryable: result.retryable.ok_or_else(|| {
                napi::Error::from_reason(
                    "PROVIDER_UNAVAILABLE: provider failure is missing retryable",
                )
            })?,
            code: result.code.ok_or_else(|| {
                napi::Error::from_reason("PROVIDER_UNAVAILABLE: provider failure is missing code")
            })?,
            message: result.message.ok_or_else(|| {
                napi::Error::from_reason(
                    "PROVIDER_UNAVAILABLE: provider failure is missing message",
                )
            })?,
        }),
        other => Err(napi::Error::from_reason(format!(
            "PROVIDER_UNAVAILABLE: unsupported provider result type `{other}`"
        ))),
    }
}

fn provider_f32(value: f64) -> Result<f32> {
    let converted = value as f32;
    if !value.is_finite() || !converted.is_finite() {
        return Err(napi::Error::from_reason(
            "PROVIDER_UNAVAILABLE: provider numeric result must be finite and representable as f32",
        ));
    }
    Ok(converted)
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderItem {
    pub key: String,
    pub text: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsEnrichmentProjection {
    pub version: u32,
    pub input_hash: String,
    pub space_id: String,
    pub memory_id: String,
    pub revision_id: String,
    pub semantic_node_id: Option<String>,
    pub content: String,
    pub max_tags: u32,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderWork {
    pub work_id: String,
    pub work_type: String,
    pub signature: String,
    pub route: JsProviderRoute,
    pub dimensions: u32,
    pub items: Vec<JsProviderItem>,
    pub query: Option<String>,
    pub candidates: Vec<String>,
    pub projection: Option<JsEnrichmentProjection>,
    pub space_policy: JsSpaceProviderPolicy,
}

impl From<NeedWork> for JsProviderWork {
    fn from(work: NeedWork) -> Self {
        match work {
            NeedWork::Embeddings(request) => Self {
                work_id: request.work_id,
                work_type: "embedding".to_owned(),
                signature: request.signature,
                route: request.route.into(),
                dimensions: u32::try_from(request.dimensions).unwrap_or(u32::MAX),
                items: request
                    .items
                    .into_iter()
                    .map(|item| JsProviderItem {
                        key: item.key,
                        text: item.text,
                    })
                    .collect(),
                query: None,
                candidates: Vec::new(),
                projection: None,
                space_policy: request.space_policy.into(),
            },
            NeedWork::Rerank(request) => Self {
                work_id: request.work_id,
                work_type: "rerank".to_owned(),
                signature: request.signature,
                route: request.route.into(),
                dimensions: 0,
                items: Vec::new(),
                query: Some(request.query),
                candidates: request.candidates,
                projection: None,
                space_policy: request.space_policy.into(),
            },
            NeedWork::Enrichment(request) => Self {
                work_id: request.work_id,
                work_type: "enrichment".to_owned(),
                signature: request.signature,
                route: request.route.into(),
                dimensions: 0,
                items: Vec::new(),
                query: None,
                candidates: Vec::new(),
                projection: Some(JsEnrichmentProjection {
                    version: request.projection.version(),
                    input_hash: digest_hex(request.projection.input_hash().as_bytes()),
                    space_id: request.projection.space_id().to_string(),
                    memory_id: request.projection.memory_id().to_string(),
                    revision_id: request.projection.revision_id().to_string(),
                    semantic_node_id: request.projection.semantic_node_id().map(ToOwned::to_owned),
                    content: request.projection.content().to_owned(),
                    max_tags: u32::try_from(request.projection.max_tags()).unwrap_or(u32::MAX),
                }),
                space_policy: request.space_policy.into(),
            },
        }
    }
}

fn query_work_to_js(work: QueryWork) -> JsQueryWork {
    match work {
        QueryWork::Embedding(request) => {
            let item = request.items.into_iter().next();
            JsQueryWork {
                r#type: "query-embedding".to_owned(),
                work_id: request.work_id,
                signature: request.signature,
                route: request.route.into(),
                input: item.map(|item| JsProviderItem {
                    key: item.key,
                    text: item.text,
                }),
                query: None,
                candidates: Vec::new(),
                space_policy: request.space_policy.into(),
            }
        }
        QueryWork::Rerank(request) => JsQueryWork {
            r#type: "query-rerank".to_owned(),
            work_id: request.work_id,
            signature: request.signature,
            route: request.route.into(),
            input: None,
            query: Some(request.query),
            candidates: request
                .candidates
                .into_iter()
                .map(|handle| JsQueryCandidate {
                    handle,
                    text: String::new(),
                })
                .collect(),
            space_policy: request.space_policy.into(),
        },
    }
}

fn response_to_js(response: memoria_runtime::RetrievalResponse) -> Result<JsQueryResponse> {
    let retrieval_id = response.retrieval_id.clone();
    let trace = trace_to_js(response.trace)?;
    Ok(JsQueryResponse {
        result_count: u32::try_from(response.results.len())
            .map_err(|error| napi::Error::from_reason(error.to_string()))?,
        authority_generation: response.snapshot.authority_generation.to_string(),
        degraded: response.execution.degraded,
        retrieval_id: retrieval_id.clone(),
        trace,
        results: response
            .results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                JsQueryResult::from((memoria_query::result_id_for(&retrieval_id, index), result))
            })
            .collect(),
    })
}

fn trace_to_js(trace: memoria_query::QueryOperatorTrace) -> Result<JsQueryTrace> {
    let candidate_counts = trace
        .candidate_counts
        .into_iter()
        .map(|(channel, count)| {
            Ok(JsQueryChannelCount {
                channel,
                count: u32::try_from(count).map_err(to_napi_error)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(JsQueryTrace {
        channels_executed: trace.channels_executed,
        candidate_counts,
        tag_basis_rank: trace
            .tag_basis_rank
            .map(u32::try_from)
            .transpose()
            .map_err(to_napi_error)?,
        tag_basis_conditioning: trace.tag_basis_conditioning.map(f64::from),
        tag_basis_explained_energy: trace.tag_basis_explained_energy.map(f64::from),
        activation_edge_visits: u32::try_from(trace.activation_edge_visits)
            .map_err(to_napi_error)?,
        activation_hops: u32::try_from(trace.activation_hops).map_err(to_napi_error)?,
        activation_truncated: trace.activation_truncated,
        diffusion_iterations: u32::try_from(trace.diffusion_iterations).map_err(to_napi_error)?,
        diffusion_convergence_delta: trace.diffusion_convergence_delta.map(f64::from),
        diffusion_truncated: trace.diffusion_truncated,
        independent_support_count: u32::try_from(trace.independent_support_count)
            .map_err(to_napi_error)?,
        correlation_suppressed_evidence: u32::try_from(trace.correlation_suppressed_evidence)
            .map_err(to_napi_error)?,
        relation_expansions: u32::try_from(trace.relation_expansions).map_err(to_napi_error)?,
        rerank_requested: trace.rerank_requested,
        rerank_applied: trace.rerank_applied,
        capability_degraded: trace.capability_degraded,
        authority_generation: trace.authority_generation.to_string(),
    })
}

pub fn query_step_to_js(step: QueryStep) -> Result<JsQueryStep> {
    match step {
        QueryStep::Complete(response) => Ok(JsQueryStep {
            state: "complete".to_owned(),
            response: Some(response_to_js(response)?),
            operation_id: None,
            work: None,
            retry_after_ms: None,
            deadline_unix_ms: None,
        }),
        QueryStep::ProviderPending { operation_id, work } => Ok(JsQueryStep {
            state: "pending".to_owned(),
            response: None,
            operation_id: Some(operation_id),
            work: Some(query_work_to_js(work)),
            retry_after_ms: None,
            deadline_unix_ms: None,
        }),
        QueryStep::ReadinessPending {
            operation_id,
            retry_after_ms,
            deadline_unix_ms,
        } => Ok(JsQueryStep {
            state: "readiness-pending".to_owned(),
            response: None,
            operation_id: Some(operation_id),
            work: None,
            retry_after_ms: Some(retry_after_ms),
            deadline_unix_ms: Some(deadline_unix_ms as f64),
        }),
    }
}

fn digest_hex(value: &[u8; 32]) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
