use memoria_query::MemoryQuery;
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
            builder = builder.text_cue(text);
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
    pub mdx: String,
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
}
