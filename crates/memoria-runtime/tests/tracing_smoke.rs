use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use memoria_query::MemoryQuery;
use memoria_runtime::{MemoriaRuntime, NeedWork, ProviderWorkResult};
use tempfile::tempdir;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record as SpanRecord};
use tracing::subscriber::Interest;
use tracing::{Event, Metadata, Subscriber};

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedRecord {
    name: String,
    target: String,
    fields: Vec<String>,
}

#[derive(Clone, Default)]
struct RecordingSubscriber {
    records: Arc<Mutex<Vec<CapturedRecord>>>,
}

static NEXT_SPAN_ID: AtomicU64 = AtomicU64::new(1);

impl Subscriber for RecordingSubscriber {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.name().starts_with("memoria.") || metadata.target().starts_with("memoria.")
    }

    fn register_callsite(&self, metadata: &'static Metadata<'static>) -> Interest {
        if self.enabled(metadata) {
            Interest::always()
        } else {
            Interest::never()
        }
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        self.records.lock().unwrap().push(CapturedRecord {
            name: attributes.metadata().name().to_owned(),
            target: attributes.metadata().target().to_owned(),
            fields: fields_from(|visitor| attributes.record(visitor)),
        });
        Id::from_u64(NEXT_SPAN_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn record(&self, _span: &Id, _values: &SpanRecord<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        self.records.lock().unwrap().push(CapturedRecord {
            name: event.metadata().name().to_owned(),
            target: event.metadata().target().to_owned(),
            fields: fields_from(|visitor| event.record(visitor)),
        });
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

fn fields_from(record: impl FnOnce(&mut FieldVisitor)) -> Vec<String> {
    let mut visitor = FieldVisitor::default();
    record(&mut visitor);
    visitor.fields
}

#[derive(Default)]
struct FieldVisitor {
    fields: Vec<String>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.fields.push(format!("{}={value:?}", field.name()));
    }
}

fn capture<T>(operation: impl FnOnce() -> T) -> (T, Vec<CapturedRecord>) {
    let subscriber = RecordingSubscriber::default();
    let records = Arc::clone(&subscriber.records);
    let result = tracing::subscriber::with_default(subscriber, operation);
    let records = records.lock().unwrap().clone();
    (result, records)
}

fn render(records: &[CapturedRecord]) -> String {
    records
        .iter()
        .map(|record| {
            format!(
                "{} {} {}",
                record.name,
                record.target,
                record.fields.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn query_execution_emits_structured_span_without_raw_mdx() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Secret Source\nRawMdxSecret")
        .unwrap();
    let query = MemoryQuery::builder()
        .spaces(vec![space])
        .text_cue("RawMdxSecret")
        .build()
        .unwrap();

    let (response, records) = capture(|| runtime.query(query).unwrap());

    assert_eq!(response.results.len(), 1);
    assert!(records.iter().any(|record| record.name == "memoria.query"));
    assert!(!render(&records).contains("RawMdxSecret"));
}

#[test]
fn derived_job_failure_event_contains_job_id_and_error_code() {
    let directory = tempdir().unwrap();
    let mut runtime = MemoriaRuntime::open(directory.path()).unwrap();
    let space = runtime.create_space("personal").unwrap();
    runtime
        .create_memory(space, Some("career"), b"# Career\nRust")
        .unwrap();
    let work = runtime.provider_poll_work().unwrap().unwrap();
    let work_id = match &work {
        NeedWork::Embeddings(request) => request.work_id.clone(),
        other => panic!("unexpected provider work: {other:?}"),
    };

    let (_, records) = capture(|| {
        runtime
            .provider_submit_result(ProviderWorkResult::Failure {
                work_id,
                retryable: false,
                code: "PROVIDER_TIMEOUT".to_owned(),
                message: "provider secret must not be logged".to_owned(),
            })
            .unwrap();
    });

    let rendered = render(&records);
    assert!(
        records
            .iter()
            .any(|record| record.target == "memoria.derived.provider_work")
    );
    assert!(rendered.contains("job_id") && rendered.contains("BJ:"));
    assert!(rendered.contains("error_code") && rendered.contains("PROVIDER_TIMEOUT"));
    assert!(!rendered.contains("provider secret must not be logged"));
}
