use std::path::Path;

use memoria_adaptive::FeedbackOutcome;
use memoria_authority::{AuthorityDb, StoreLayout};
use memoria_query::MemoryQuery;
use memoria_runtime::{FeedbackSubmission, FeedbackSubmissionEvent, MemoriaRuntime, PurgeState};
use rusqlite::{Connection, params};
use tempfile::tempdir;

fn authority_database(store: &Path) -> std::path::PathBuf {
    StoreLayout::open(store)
        .unwrap()
        .authority_database()
        .to_path_buf()
}

fn adaptive_database(store: &Path) -> std::path::PathBuf {
    store.join("adaptive").join("adaptive.sqlite")
}

#[test]
fn purge_plan_survives_runtime_reopen() {
    let directory = tempdir().unwrap();
    let (memory_id, plan_id) = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        let memory_id = runtime
            .create_memory(space, Some("planned"), b"# Planned\nkeep for now")
            .unwrap();
        let plan = runtime.plan_purge(memory_id).unwrap();
        (memory_id, plan.id)
    };

    let mut reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let restored = reopened.plan_purge(memory_id).unwrap();
    assert_eq!(restored.id, plan_id);
    assert_eq!(restored.state, PurgeState::Planned);

    let authority = AuthorityDb::open(authority_database(directory.path())).unwrap();
    let journal = authority.list_purge_operations().unwrap();
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].purge_id, plan_id);
    assert_eq!(journal[0].state, "planned");
}

#[test]
fn committed_or_cleaning_purge_resumes_on_open() {
    let directory = tempdir().unwrap();
    let (committed_memory, cleaning_memory, committed_plan, cleaning_plan) = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        let committed_memory = runtime
            .create_memory(space, Some("committed"), b"# Committed\nresume")
            .unwrap();
        let cleaning_memory = runtime
            .create_memory(space, Some("cleaning"), b"# Cleaning\nresume")
            .unwrap();
        let committed_plan = runtime.plan_purge(committed_memory).unwrap();
        let cleaning_plan = runtime.plan_purge(cleaning_memory).unwrap();
        (
            committed_memory,
            cleaning_memory,
            committed_plan.id,
            cleaning_plan.id,
        )
    };

    let authority = AuthorityDb::open(authority_database(directory.path())).unwrap();
    authority
        .transition_purge_operation(&committed_plan, "planned", "committed")
        .unwrap();
    authority
        .transition_purge_operation(&cleaning_plan, "planned", "committed")
        .unwrap();
    authority.purge_memory(cleaning_memory).unwrap();
    authority
        .transition_purge_operation(&cleaning_plan, "committed", "cleaning")
        .unwrap();
    drop(authority);

    let _reopened = MemoriaRuntime::open(directory.path()).unwrap();
    let authority = AuthorityDb::open(authority_database(directory.path())).unwrap();
    assert!(authority.get_memory(committed_memory).is_err());
    assert!(authority.get_memory(cleaning_memory).is_err());
    assert!(authority.list_purge_operations().unwrap().is_empty());

    let connection = Connection::open(authority_database(directory.path())).unwrap();
    for plan_id in [committed_plan, cleaning_plan] {
        let state: String = connection
            .query_row(
                "SELECT state FROM purge_operations WHERE purge_id = ?1",
                [plan_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "completed");
    }
}

#[test]
fn completed_purge_has_no_target_authority_rows() {
    let directory = tempdir().unwrap();
    let memory_id = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        let memory_id = runtime
            .create_memory(space, Some("purge"), b"# Purge\nremove authority rows")
            .unwrap();
        let plan = runtime.plan_purge(memory_id).unwrap();
        assert_eq!(
            runtime.execute_purge(&plan.id).unwrap().state,
            PurgeState::Completed
        );
        memory_id
    };

    let connection = Connection::open(authority_database(directory.path())).unwrap();
    for table in [
        "memories",
        "memory_state_history",
        "revisions",
        "idempotency_records",
    ] {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE memory_id = ?1"),
                params![memory_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap_or_else(|error| {
                if table == "memories" {
                    panic!("unexpected authority query failure: {error}");
                }
                0
            });
        assert_eq!(count, 0, "target rows remain in {table}");
    }
}

#[test]
fn completed_purge_has_no_target_adaptive_events_or_materialized_rows() {
    let directory = tempdir().unwrap();
    let memory_id = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        let memory_id = runtime
            .create_memory(space, Some("adaptive"), b"# Adaptive\nremember this")
            .unwrap();
        let response = runtime
            .query(
                MemoryQuery::builder()
                    .spaces(vec![space])
                    .text_cue("remember")
                    .build()
                    .unwrap(),
            )
            .unwrap();
        runtime
            .submit_feedback(FeedbackSubmission {
                retrieval_id: response.retrieval_id.clone(),
                idempotency_key: "purge-feedback".to_owned(),
                events: vec![FeedbackSubmissionEvent {
                    result_id: format!("{}:result:0", response.retrieval_id),
                    outcome: FeedbackOutcome::Used,
                }],
            })
            .unwrap();
        assert_eq!(
            runtime.plan_purge(memory_id).unwrap().state,
            PurgeState::Planned
        );
        let plan = runtime.plan_purge(memory_id).unwrap();
        runtime.execute_purge(&plan.id).unwrap();
        memory_id
    };

    let log =
        memoria_adaptive::AdaptiveEventLog::open(adaptive_database(directory.path())).unwrap();
    assert!(
        log.events()
            .iter()
            .all(|event| event.memory_id != memory_id)
    );
    let connection = Connection::open(adaptive_database(directory.path())).unwrap();
    for table in [
        "adaptive_events",
        "adaptive_familiarity",
        "adaptive_tag_affinity",
        "adaptive_query_class_affinity",
    ] {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE memory_id = ?1"),
                params![memory_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "target rows remain in {table}");
    }
}

#[test]
fn unshared_unreachable_source_blob_is_removed_but_shared_blob_is_retained() {
    let directory = tempdir().unwrap();
    let (shared_one, shared_two, unique) = {
        let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
        let space = runtime.create_space("personal").unwrap();
        let shared_one = runtime
            .create_memory(space, Some("shared-one"), b"# Shared\nsame source")
            .unwrap();
        let shared_two = runtime
            .create_memory(space, Some("shared-two"), b"# Shared\nsame source")
            .unwrap();
        let unique = runtime
            .create_memory(space, Some("unique"), b"# Unique\nremove")
            .unwrap();
        (shared_one, shared_two, unique)
    };

    let object_dir = directory.path().join("authority").join("objects");
    assert_eq!(std::fs::read_dir(&object_dir).unwrap().count(), 2);
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let shared_plan = runtime.plan_purge(shared_one).unwrap();
    runtime.execute_purge(&shared_plan.id).unwrap();
    assert_eq!(std::fs::read_dir(&object_dir).unwrap().count(), 2);

    let unique_plan = runtime.plan_purge(unique).unwrap();
    runtime.execute_purge(&unique_plan.id).unwrap();
    assert_eq!(std::fs::read_dir(&object_dir).unwrap().count(), 1);

    let shared_plan = runtime.plan_purge(shared_two).unwrap();
    runtime.execute_purge(&shared_plan.id).unwrap();
    assert_eq!(std::fs::read_dir(&object_dir).unwrap().count(), 0);
}
