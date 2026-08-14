use memoria_napi::convert::{
    JsQueryAuthority, JsQueryBudget, JsQueryConsistency, JsQueryConstraints, JsQueryCue,
    JsQueryHistory, JsQueryMemoryReference, JsQueryRequest, JsQueryTemporal, QueryRequest,
};
use memoria_query::{AuthorityConsistency, ReadinessBehavior};
use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};
use std::time::Duration;

fn full_request() -> JsQueryRequest {
    let memory_id = MemoryId::from_bytes([2; 16]).to_string();
    let revision_id = RevisionId::from_bytes([3; 32]).to_string();
    JsQueryRequest {
        scope: vec![SpaceId::from_bytes([1; 16]).to_string()],
        cue: Some(JsQueryCue {
            text: Some("career".to_owned()),
            tags: Some(vec!["work".to_owned()]),
            entities: Some(vec!["person:alice".to_owned()]),
            memories: Some(vec![JsQueryMemoryReference {
                memory_id: memory_id.clone(),
                revision_id: Some(revision_id.clone()),
                node_id: Some("node-career".to_owned()),
            }]),
        }),
        constraints: Some(JsQueryConstraints {
            tags: Some(vec!["must-include".to_owned()]),
            entities: Some(vec!["project:memoria".to_owned()]),
            memories: Some(vec![JsQueryMemoryReference {
                memory_id,
                revision_id: None,
                node_id: None,
            }]),
            lifecycle: Some("active".to_owned()),
        }),
        temporal: Some(JsQueryTemporal {
            valid_at: Some("2026-08-01".to_owned()),
        }),
        history: Some(JsQueryHistory {
            mode: "changes".to_owned(),
            from_authority_generation: Some("2".to_owned()),
            to_authority_generation: Some("7".to_owned()),
        }),
        consistency: JsQueryConsistency {
            authority: JsQueryAuthority {
                mode: "at-least".to_owned(),
                generation: Some("5".to_owned()),
            },
            required: vec!["semantic".to_owned()],
            preferred: vec!["reranking".to_owned()],
            on_not_ready: "wait".to_owned(),
            timeout_ms: 9_000,
        },
        budget: JsQueryBudget {
            max_results: 7,
            max_matches_per_result: 4,
            max_evidence_tokens: 321,
        },
        quality: "thorough".to_owned(),
    }
}

#[test]
fn text_only_native_query_has_no_semantic_capability() {
    let mut request = JsQueryRequest::fixture();
    request.cue = Some(JsQueryCue {
        text: Some("career".to_owned()),
        tags: None,
        entities: None,
        memories: None,
    });
    request.constraints = None;
    request.temporal = None;
    request.history = None;
    request.consistency = JsQueryConsistency::default();
    request.budget = JsQueryBudget::default();
    request.quality = "balanced".to_owned();

    let query = QueryRequest::try_from(request)
        .unwrap()
        .into_core()
        .unwrap();

    assert_eq!(query.cue.text, vec!["career"]);
    assert!(query.required_capabilities.is_empty());
    assert!(query.preferred_capabilities.is_empty());
}

#[test]
fn cue_and_constraints_round_trip_without_merging() {
    let request = full_request();
    let core = QueryRequest::try_from(request.clone())
        .unwrap()
        .into_core()
        .unwrap();

    assert_eq!(core.cue.tags, vec!["work"]);
    assert_eq!(core.constraints.tags, vec!["must-include"]);
    assert_eq!(core.cue.entities[0].as_str(), "person:alice");
    assert_eq!(core.constraints.entities[0].as_str(), "project:memoria");
    assert_ne!(core.cue.memories, core.constraints.memories);

    let round_trip = JsQueryRequest::from(QueryRequest::try_from(request).unwrap());
    assert_eq!(round_trip, full_request());
}

#[test]
fn history_temporal_consistency_and_budget_round_trip() {
    let request = full_request();
    let query = QueryRequest::try_from(request.clone())
        .unwrap()
        .into_core()
        .unwrap();

    assert_eq!(
        query.constraints.valid_at.as_ref().unwrap().to_string(),
        "2026-08-01"
    );
    assert_eq!(query.history.mode.to_string(), "changes");
    assert_eq!(query.history.from_authority_generation.unwrap().value(), 2);
    assert_eq!(query.history.to_authority_generation.unwrap().value(), 7);
    assert_eq!(
        query.consistency.authority,
        AuthorityConsistency::AtLeast(AuthorityGeneration::new(5))
    );
    assert_eq!(
        query.consistency.readiness,
        ReadinessBehavior::Wait(Duration::from_secs(9))
    );
    assert_eq!(query.budget.max_results, 7);
    assert_eq!(query.budget.max_matches_per_result, 4);
    assert_eq!(query.budget.max_evidence_tokens, 321);
    assert_eq!(query.quality.to_string(), "thorough");
}
