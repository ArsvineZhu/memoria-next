use memoria_derived::{DerivedCatalog, TagProvenance};
use memoria_runtime::{EmbeddingVector, MemoriaRuntime, NeedWork, ProviderWorkResult};
use memoria_types::SpaceId;
use tempfile::tempdir;

fn drain_to_enrichment(runtime: &mut MemoriaRuntime) -> memoria_runtime::EnrichmentBatchRequest {
    loop {
        match runtime.provider_poll_work().unwrap().unwrap() {
            NeedWork::Embeddings(request) => {
                let work_id = request.work_id.clone();
                runtime
                    .provider_submit_result(ProviderWorkResult::Embeddings {
                        work_id,
                        vectors: request
                            .items
                            .into_iter()
                            .map(|item| EmbeddingVector {
                                key: item.key,
                                values: vec![1.0; 64],
                            })
                            .collect(),
                    })
                    .unwrap();
            }
            NeedWork::Enrichment(request) => return request,
            NeedWork::Rerank(_) => panic!("unexpected rerank work"),
        }
    }
}

fn catalog(runtime: &MemoriaRuntime) -> DerivedCatalog {
    DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap()
}

#[test]
fn generated_tags_survive_runtime_restart() {
    let directory = tempdir().unwrap();
    let space;
    {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        space = runtime.create_space("personal").unwrap();
        runtime
            .create_memory(space, Some("one"), b"# One\nRust")
            .unwrap();
        let request = drain_to_enrichment(&mut runtime);
        runtime
            .provider_submit_result(ProviderWorkResult::Enrichment {
                work_id: request.work_id,
                tags: vec![" Systems ".to_owned()],
            })
            .unwrap();
        assert_eq!(catalog(&runtime).tag_memberships().unwrap().len(), 1);
    }
    let reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let memberships = catalog(&reopened).tag_memberships().unwrap();
    assert_eq!(memberships.len(), 1);
    assert_eq!(memberships[0].space_id, space);
    assert_eq!(memberships[0].provenance, TagProvenance::Generated);
}

#[test]
fn same_normalized_tag_has_one_store_global_tag_id() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let first = runtime.create_space("first").unwrap();
    let second = runtime.create_space("second").unwrap();
    runtime
        .create_memory(first, Some("one"), b"# One\nfirst")
        .unwrap();
    let request = drain_to_enrichment(&mut runtime);
    runtime
        .provider_submit_result(ProviderWorkResult::Enrichment {
            work_id: request.work_id,
            tags: vec!["Systems".to_owned()],
        })
        .unwrap();
    runtime
        .create_memory(second, Some("two"), b"# Two\nsecond")
        .unwrap();
    let request = drain_to_enrichment(&mut runtime);
    runtime
        .provider_submit_result(ProviderWorkResult::Enrichment {
            work_id: request.work_id,
            tags: vec![" systems ".to_owned()],
        })
        .unwrap();
    let catalog = catalog(&runtime);
    assert_eq!(catalog.tag_dictionary_entries().unwrap().len(), 1);
    let memberships = catalog.tag_memberships().unwrap();
    assert_eq!(memberships.len(), 2);
    assert_ne!(memberships[0].space_id, memberships[1].space_id);
}

#[test]
fn same_tag_keeps_separate_space_membership_evidence() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let spaces = [
        runtime.create_space("first").unwrap(),
        runtime.create_space("second").unwrap(),
    ];
    for (index, space) in spaces.into_iter().enumerate() {
        runtime
            .create_memory(space, Some(&format!("memory-{index}")), b"# Note\ntext")
            .unwrap();
        let request = drain_to_enrichment(&mut runtime);
        runtime
            .provider_submit_result(ProviderWorkResult::Enrichment {
                work_id: request.work_id,
                tags: vec!["shared".to_owned()],
            })
            .unwrap();
    }
    let memberships = catalog(&runtime).tag_memberships().unwrap();
    assert_eq!(memberships.len(), 2);
    assert_eq!(
        memberships
            .iter()
            .map(|membership| membership.space_id)
            .collect::<std::collections::BTreeSet<SpaceId>>()
            .len(),
        2
    );
}

#[test]
fn generated_tag_publication_does_not_change_authority_or_content_vector_payload() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("one"), b"# One\ncontent")
        .unwrap();
    let request = drain_to_enrichment(&mut runtime);
    let before_payloads = catalog(&runtime).vector_payload_count().unwrap();
    let before_generation = runtime.status().authority_generation;
    runtime
        .provider_submit_result(ProviderWorkResult::Enrichment {
            work_id: request.work_id,
            tags: vec!["generated".to_owned()],
        })
        .unwrap();
    let after = catalog(&runtime);
    assert_eq!(after.vector_payload_count().unwrap(), before_payloads);
    assert_eq!(runtime.status().authority_generation, before_generation);
    assert_eq!(memory, after.tag_memberships().unwrap()[0].memory_id);
}

#[test]
fn generated_and_explicit_provenance_remain_distinct() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(
            space,
            Some("one"),
            br#"# One
<Tag value="project"/>
content
"#,
        )
        .unwrap();
    let request = drain_to_enrichment(&mut runtime);
    runtime
        .provider_submit_result(ProviderWorkResult::Enrichment {
            work_id: request.work_id,
            tags: vec!["Project".to_owned()],
        })
        .unwrap();
    let provenances = catalog(&runtime)
        .tag_memberships()
        .unwrap()
        .into_iter()
        .map(|membership| membership.provenance)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        provenances,
        [TagProvenance::Explicit, TagProvenance::Generated]
            .into_iter()
            .collect()
    );
}
