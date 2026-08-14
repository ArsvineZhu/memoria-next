use memoria_derived::DerivedCatalog;
use memoria_runtime::{EmbeddingVector, MemoriaRuntime, MemoryQuery, NeedWork, ProviderWorkResult};
use tempfile::tempdir;

fn take_embedding_work(runtime: &mut MemoriaRuntime) -> memoria_runtime::EmbeddingBatchRequest {
    match runtime.provider_poll_work().unwrap().unwrap() {
        NeedWork::Embeddings(request) => request,
        other => panic!("expected embedding work, got {other:?}"),
    }
}

fn submit_embedding(runtime: &mut MemoriaRuntime, work: memoria_runtime::EmbeddingBatchRequest) {
    runtime
        .provider_submit_result(ProviderWorkResult::Embeddings {
            work_id: work.work_id,
            vectors: work
                .items
                .into_iter()
                .map(|item| EmbeddingVector {
                    key: item.key,
                    values: vec![1.0, 0.0, 0.0],
                })
                .collect(),
        })
        .unwrap();
}

fn serving_manifest(runtime: &MemoriaRuntime) -> memoria_derived::DerivedManifest {
    DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite"))
        .unwrap()
        .serving_manifest()
        .unwrap()
        .unwrap()
}

fn semantic_membership_revision(runtime: &MemoriaRuntime) -> memoria_types::RevisionId {
    let catalog = DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap();
    let manifest = catalog.serving_manifest().unwrap().unwrap();
    let artifact_id = manifest
        .artifacts()
        .find(|id| catalog.artifact(*id).unwrap().kind() == "semantic")
        .expect("semantic artifact should be serving");
    catalog
        .vector_memberships_for_artifact(artifact_id)
        .unwrap()
        .into_iter()
        .next()
        .expect("semantic membership should be serving")
        .revision_id
}

fn semantic_membership_space(runtime: &MemoriaRuntime) -> memoria_types::SpaceId {
    let catalog = DerivedCatalog::open(runtime.data_dir().join("derived/catalog.sqlite")).unwrap();
    let manifest = catalog.serving_manifest().unwrap().unwrap();
    let artifact_id = manifest
        .artifacts()
        .find(|id| catalog.artifact(*id).unwrap().kind() == "semantic")
        .expect("semantic artifact should be serving");
    catalog
        .vector_memberships_for_artifact(artifact_id)
        .unwrap()
        .into_iter()
        .next()
        .expect("semantic membership should be serving")
        .space_id
}

#[test]
fn old_embedding_completion_publishes_after_tag_only_new_generation() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("note"), b"# Note\nstable semantic text\n")
        .unwrap();
    let old_work = take_embedding_work(&mut runtime);
    let head = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("stable")
                .build()
                .unwrap(),
        )
        .unwrap()
        .results[0]
        .revision_id;

    let mutation = runtime
        .revise_memory(
            memory,
            head,
            b"# Note\nstable semantic text\n<Tag value=\"review\"/>\n",
        )
        .unwrap();
    submit_embedding(&mut runtime, old_work);

    let manifest = serving_manifest(&runtime);
    assert_eq!(manifest.authority_generation(), mutation.generation);
    assert!(manifest.capability("semantic").is_ready());
    assert_eq!(semantic_membership_revision(&runtime), mutation.revision_id);
}

#[test]
fn old_embedding_completion_publishes_after_valid_to_only_new_generation() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(
            space,
            Some("note"),
            b"# Note\n<State id=\"s\" validFrom=\"2025\">stable semantic text</State>\n",
        )
        .unwrap();
    let old_work = take_embedding_work(&mut runtime);
    let head = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("stable")
                .build()
                .unwrap(),
        )
        .unwrap()
        .results[0]
        .revision_id;

    let mutation = runtime
        .revise_memory(
            memory,
            head,
            b"# Note\n<State id=\"s\" validFrom=\"2025\" validTo=\"2026\">stable semantic text</State>\n",
        )
        .unwrap();
    submit_embedding(&mut runtime, old_work);

    let manifest = serving_manifest(&runtime);
    assert_eq!(manifest.authority_generation(), mutation.generation);
    assert!(manifest.capability("semantic").is_ready());
    assert_eq!(semantic_membership_revision(&runtime), mutation.revision_id);
}

#[test]
fn old_embedding_completion_publishes_after_space_move_with_same_projection() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let source_space = runtime.create_space("source").unwrap();
    let target_space = runtime.create_space("target").unwrap();
    let memory = runtime
        .create_memory(
            source_space,
            Some("note"),
            b"# Note\nstable semantic text\n",
        )
        .unwrap();
    let old_work = take_embedding_work(&mut runtime);
    let expected_generation = runtime.status().authority_generation;

    let mutation = runtime
        .move_memory(memory, target_space, Some("note"), expected_generation)
        .unwrap();
    submit_embedding(&mut runtime, old_work);

    let manifest = serving_manifest(&runtime);
    assert_eq!(manifest.authority_generation(), mutation.generation);
    assert!(manifest.capability("semantic").is_ready());
    assert_eq!(semantic_membership_space(&runtime), target_space);
}

#[test]
fn old_embedding_completion_is_superseded_after_visible_text_change() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("note"), b"# Note\nold semantic text\n")
        .unwrap();
    let old_work = take_embedding_work(&mut runtime);
    let head = runtime
        .query(
            MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("old")
                .build()
                .unwrap(),
        )
        .unwrap()
        .results[0]
        .revision_id;

    runtime
        .revise_memory(memory, head, b"# Note\nnew semantic text\n")
        .unwrap();
    submit_embedding(&mut runtime, old_work);

    let manifest = serving_manifest(&runtime);
    assert!(!manifest.capability("semantic").is_ready());
}
