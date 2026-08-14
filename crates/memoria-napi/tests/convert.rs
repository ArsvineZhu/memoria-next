use memoria_napi::convert::{JsQueryRequest, QueryRequest};

#[test]
fn query_request_round_trip_keeps_public_semantics() {
    let js = JsQueryRequest::fixture();
    let core = QueryRequest::try_from(js.clone()).unwrap();
    let back = JsQueryRequest::from(core);
    assert_eq!(back.scope, js.scope);
    assert_eq!(back.cue, js.cue);
    assert_eq!(back.constraints, js.constraints);
    assert_eq!(back.temporal, js.temporal);
    assert_eq!(back.history, js.history);
    assert_eq!(back.consistency, js.consistency);
    assert_eq!(back.budget, js.budget);
    assert_eq!(back.quality, js.quality);
}
