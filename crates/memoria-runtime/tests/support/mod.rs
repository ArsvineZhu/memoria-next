use memoria_derived::DerivedCatalog;
use memoria_runtime::{EmbeddingVector, MemoriaRuntime, NeedWork, ProviderWorkResult};

#[allow(dead_code)]
pub fn embedding_vector(prefix: &[f32]) -> Vec<f32> {
    let mut values = prefix.to_vec();
    values.resize(64, 0.0);
    values
}

#[allow(dead_code)]
pub fn drain_background_work<F>(runtime: &mut MemoriaRuntime, mut vector_for: F)
where
    F: FnMut(&str) -> Vec<f32>,
{
    while let Some(work) = runtime.provider_poll_work().unwrap() {
        match work {
            NeedWork::Embeddings(request) => {
                let vectors = request
                    .items
                    .iter()
                    .map(|item| EmbeddingVector {
                        key: item.key.clone(),
                        values: vector_for(&item.key),
                    })
                    .collect();
                runtime
                    .provider_submit_result(ProviderWorkResult::Embeddings {
                        work_id: request.work_id,
                        vectors,
                    })
                    .unwrap();
            }
            NeedWork::Enrichment(request) => runtime
                .provider_submit_result(ProviderWorkResult::Enrichment {
                    work_id: request.work_id,
                    tags: Vec::new(),
                })
                .unwrap(),
            NeedWork::Rerank(request) => {
                panic!("unexpected background rerank work: {}", request.work_id)
            }
        }
    }
}

#[allow(dead_code)]
pub fn publish_manifest_with_capabilities(
    runtime: &MemoriaRuntime,
    additional_capabilities: &[&str],
) {
    let mut catalog =
        DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap();
    let manifest = catalog.serving_manifest().unwrap().unwrap();
    let mut capabilities = manifest
        .capabilities()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for capability in additional_capabilities {
        if !capabilities.iter().any(|item| item == capability) {
            capabilities.push((*capability).to_owned());
        }
    }
    catalog
        .publish_manifest_rebased_at_generation(
            manifest.artifacts().collect(),
            runtime.status().authority_generation,
            capabilities,
        )
        .unwrap();
}
