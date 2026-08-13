use memoria_napi::convert::{JsQueryRequest, QueryRequest};

#[test]
fn query_request_round_trip_keeps_public_semantics_only() {
    let js = JsQueryRequest::fixture();
    let core = QueryRequest::try_from(js.clone()).unwrap();
    let back = JsQueryRequest::from(core);
    assert_eq!(back.scope, js.scope);
    assert_eq!(back.text, js.text);
}
