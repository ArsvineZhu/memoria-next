use crate::DerivedError;
use crate::dependency::{ProjectionInputHash, ProjectionKind};

pub const RERANK_VIEW_PROJECTION_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RerankViewV1 {
    handle: String,
    content: String,
    input_hash: ProjectionInputHash,
    producer_signature: String,
}

impl RerankViewV1 {
    pub fn build(
        handle: impl Into<String>,
        document_title: impl Into<String>,
        section_path: impl Into<String>,
        match_text: impl Into<String>,
        neighboring_context: impl Into<String>,
        producer_signature: impl Into<String>,
    ) -> Result<Self, DerivedError> {
        let handle = handle.into();
        let producer_signature = producer_signature.into();
        if handle.trim().is_empty() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "rerank view handle must not be empty".to_owned(),
            });
        }
        if producer_signature.trim().is_empty() {
            return Err(DerivedError::InvalidProjectionValue {
                value: "rerank view producer signature must not be empty".to_owned(),
            });
        }
        let content = [
            document_title.into(),
            section_path.into(),
            match_text.into(),
            neighboring_context.into(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let bytes = canonical_bytes(&handle, &content);
        let input_hash =
            ProjectionInputHash::new(ProjectionKind::RerankView, &bytes, &producer_signature);
        Ok(Self {
            handle,
            content,
            input_hash,
            producer_signature,
        })
    }

    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub const fn input_hash(&self) -> &ProjectionInputHash {
        &self.input_hash
    }

    #[must_use]
    pub fn producer_signature(&self) -> &str {
        &self.producer_signature
    }

    #[must_use]
    pub const fn version() -> u32 {
        RERANK_VIEW_PROJECTION_VERSION
    }
}

fn canonical_bytes(handle: &str, content: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(handle.len() + content.len() + 32);
    bytes.extend_from_slice(b"memoria-rerank-view-v1\0");
    put_string(&mut bytes, handle);
    put_string(&mut bytes, content);
    bytes
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}
