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

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderItem {
    pub key: String,
    pub text: String,
}

#[napi(object)]
#[derive(Clone, Debug, PartialEq)]
pub struct JsProviderWork {
    pub work_id: String,
    pub work_type: String,
    pub signature: String,
    pub items: Vec<JsProviderItem>,
    pub query: Option<String>,
    pub candidates: Vec<String>,
    pub text: Option<String>,
}

impl From<NeedWork> for JsProviderWork {
    fn from(work: NeedWork) -> Self {
        match work {
            NeedWork::Embeddings(request) => Self {
                work_id: request.work_id,
                work_type: "embedding".to_owned(),
                signature: request.signature,
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
                text: None,
            },
            NeedWork::Rerank(request) => Self {
                work_id: request.work_id,
                work_type: "rerank".to_owned(),
                signature: request.signature,
                items: Vec::new(),
                query: Some(request.query),
                candidates: request.candidates,
                text: None,
            },
            NeedWork::Enrichment(request) => Self {
                work_id: request.work_id,
                work_type: "enrichment".to_owned(),
                signature: request.signature,
                items: Vec::new(),
                query: None,
                candidates: Vec::new(),
                text: Some(request.text),
            },
        }
    }
}
