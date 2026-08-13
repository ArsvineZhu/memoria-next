use memoria_query::MemoryQuery;
use memoria_runtime::NeedWork;
use memoria_types::SpaceId;
use napi::bindgen_prelude::Result;
use napi_derive::napi;

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryRequest {
    pub scope: Vec<String>,
    pub text: Option<String>,
}

impl JsQueryRequest {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            scope: vec![SpaceId::from_bytes([1; 16]).to_string()],
            text: Some("career".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRequest {
    pub scope: Vec<SpaceId>,
    pub text: Option<String>,
}

impl QueryRequest {
    pub fn into_core(self) -> Result<MemoryQuery> {
        let mut builder = MemoryQuery::builder().spaces(self.scope);
        if let Some(text) = self.text {
            builder = builder.text_cue(text).prefer_capability("semantic");
        }
        builder
            .build()
            .map_err(|error| napi::Error::from_reason(error.to_string()))
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
                    .map_err(|error| napi::Error::from_reason(error.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            scope,
            text: value.text,
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
            text: value.text,
        }
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
    pub active_read_leases: u32,
    pub closed: bool,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsQueryResponse {
    pub result_count: u32,
    pub authority_generation: String,
    pub degraded: bool,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderResult {
    pub work_id: String,
    pub accepted: bool,
    pub tags: Option<Vec<String>>,
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
    pub dimensions: u32,
    pub items: Vec<JsProviderItem>,
    pub query: Option<String>,
    pub candidates: Vec<String>,
    pub projection: Option<JsEnrichmentProjection>,
}

impl From<NeedWork> for JsProviderWork {
    fn from(work: NeedWork) -> Self {
        match work {
            NeedWork::Embeddings(request) => Self {
                work_id: request.work_id,
                work_type: "embedding".to_owned(),
                signature: request.signature,
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
            },
            NeedWork::Rerank(request) => Self {
                work_id: request.work_id,
                work_type: "rerank".to_owned(),
                signature: request.signature,
                dimensions: 0,
                items: Vec::new(),
                query: Some(request.query),
                candidates: request.candidates,
                projection: None,
            },
            NeedWork::Enrichment(request) => Self {
                work_id: request.work_id,
                work_type: "enrichment".to_owned(),
                signature: request.signature,
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
            },
        }
    }
}

fn digest_hex(value: &[u8; 32]) -> String {
    let mut output = String::with_capacity(value.len() * 2);
    for byte in value {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
