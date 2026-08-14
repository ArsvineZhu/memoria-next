use memoria_runtime::{MemoriaRuntime, NeedWork};
use tempfile::tempdir;

fn embedding_work_id(work: NeedWork) -> String {
    match work {
        NeedWork::Embeddings(request) => request.work_id,
        other => panic!("expected embedding work, got {other:?}"),
    }
}

#[test]
fn queued_embedding_job_survives_runtime_reopen() {
    let directory = tempdir().unwrap();
    let space;
    {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        space = runtime.create_space("personal").unwrap();
        runtime
            .create_memory(space, Some("one"), b"# One\nqueued work")
            .unwrap();
        assert!(runtime.provider_poll_work().unwrap().is_some());
    }

    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let work = reopened.provider_poll_work().unwrap().unwrap();
    assert!(matches!(work, NeedWork::Embeddings(_)));
}

#[test]
fn running_job_is_requeued_and_not_lost_after_reopen() {
    let directory = tempdir().unwrap();
    {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        runtime
            .create_memory(space, Some("one"), b"# One\nrunning work")
            .unwrap();
        let work = runtime.provider_poll_work().unwrap().unwrap();
        assert!(matches!(work, NeedWork::Embeddings(_)));
    }

    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    assert!(matches!(
        reopened.provider_poll_work().unwrap(),
        Some(NeedWork::Embeddings(_))
    ));
}

#[test]
fn authority_ahead_of_manifest_reconstructs_missing_build_intent() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("one"), b"# One\nfirst")
        .unwrap();
    let first = embedding_work_id(runtime.provider_poll_work().unwrap().unwrap());
    assert!(first.contains("_2"));
    drop(runtime);

    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let work = reopened.provider_poll_work().unwrap().unwrap();
    assert!(matches!(work, NeedWork::Embeddings(_)));
}

#[test]
fn superseded_revision_work_is_not_emitted_after_new_revision_is_queued() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    let memory = runtime
        .create_memory(space, Some("one"), b"# One\n<State id=\"s\">first</State>")
        .unwrap();
    let old_work = embedding_work_id(runtime.provider_poll_work().unwrap().unwrap());
    let head = runtime
        .query(
            memoria_query::MemoryQuery::builder()
                .spaces(vec![space])
                .text_cue("first")
                .build()
                .unwrap(),
        )
        .unwrap()
        .results[0]
        .revision_id;
    let mutation = runtime
        .revise_memory(memory, head, b"# One\n<State id=\"s\">second</State>")
        .unwrap();
    let new_work = loop {
        let work = runtime.provider_poll_work().unwrap().unwrap();
        match work {
            NeedWork::Embeddings(request) => break request.work_id,
            NeedWork::Enrichment(request) => {
                runtime
                    .provider_submit_result(memoria_runtime::ProviderWorkResult::Enrichment {
                        work_id: request.work_id,
                        tags: Vec::new(),
                    })
                    .unwrap();
            }
            NeedWork::Rerank(_) => unreachable!(),
        }
    };
    assert_ne!(old_work, new_work);
    assert!(new_work.contains(&mutation.generation.to_string()));
}
