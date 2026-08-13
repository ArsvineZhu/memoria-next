use memoria_derived::EnrichmentProjection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingItem {
    pub key: String,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddingBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub dimensions: usize,
    pub items: Vec<EmbeddingItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RerankBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub query: String,
    pub candidates: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnrichmentBatchRequest {
    pub work_id: String,
    pub signature: String,
    pub projection: EnrichmentProjection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeedWork {
    Embeddings(EmbeddingBatchRequest),
    Rerank(RerankBatchRequest),
    Enrichment(EnrichmentBatchRequest),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RerankScore {
    pub handle: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderWorkResult {
    pub work_id: String,
    pub accepted: bool,
    pub scores: Vec<RerankScore>,
    pub tags: Vec<String>,
}
