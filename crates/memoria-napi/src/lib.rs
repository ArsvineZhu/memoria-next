pub mod convert;
mod error;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use memoria_authority::SpaceProviderPolicy;
use memoria_query::{MemoryQuery, ReadSession};
use memoria_runtime::MemoriaRuntime;
use memoria_types::{MemoryId, RevisionId, SpaceId};
use napi::bindgen_prelude::Result;
use napi_derive::napi;

use crate::convert::{
    JsBackupResult, JsCreateMemoryRequest, JsFeedbackCommit, JsFeedbackSubmission,
    JsMemoryMutation, JsPortableImportRequest, JsPortableImportResult, JsPortableMemory,
    JsProviderResult, JsProviderWork, JsPurgePlan, JsQueryRequest, JsQueryStep,
    JsReviseMemoryRequest, JsSpaceProviderPolicy, JsStatus, QueryRequest, provider_result_from_js,
    query_step_to_js,
};
use crate::error::{runtime_error, to_napi_error};

#[napi]
pub struct NativeStore {
    runtime: Mutex<Option<MemoriaRuntime>>,
    sessions: Mutex<HashMap<String, ReadSession>>,
    next_session: Mutex<u64>,
}

impl NativeStore {
    fn runtime(&self) -> Result<MutexGuard<'_, Option<MemoriaRuntime>>> {
        self.runtime
            .lock()
            .map_err(|_| napi::Error::from_reason("runtime mutex poisoned"))
    }
}

#[napi]
impl NativeStore {
    #[napi]
    pub fn close(&self) -> Result<()> {
        let mut runtime = self.runtime()?;
        if let Some(runtime) = runtime.as_mut() {
            runtime.close().map_err(runtime_error)?;
        }
        *runtime = None;
        Ok(())
    }

    #[napi]
    pub fn status(&self) -> Result<JsStatus> {
        let runtime = self.runtime()?;
        let runtime = runtime
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
        let status = runtime.status();
        Ok(JsStatus {
            authority_generation: status.authority_generation.to_string(),
            base_coverage: status.base_coverage.to_string(),
            semantic_coverage: status.semantic_coverage.to_string(),
            semantic_build_coverage: status.semantic_build_coverage.to_string(),
            active_read_leases: u32::try_from(status.active_read_leases).map_err(to_napi_error)?,
            closed: status.closed,
        })
    }
}

#[napi]
pub fn open_store(data_dir: String) -> Result<NativeStore> {
    let runtime = MemoriaRuntime::open(data_dir).map_err(runtime_error)?;
    Ok(NativeStore {
        runtime: Mutex::new(Some(runtime)),
        sessions: Mutex::new(HashMap::new()),
        next_session: Mutex::new(0),
    })
}

#[napi]
pub fn close_store(store: &NativeStore) -> Result<()> {
    store.close()
}

#[napi]
pub fn authority_create_space(
    store: &NativeStore,
    space_key: String,
    provider_policy: Option<JsSpaceProviderPolicy>,
) -> Result<String> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let provider_policy = provider_policy
        .map(SpaceProviderPolicy::try_from)
        .transpose()?;
    runtime
        .create_space_with_policy(space_key, provider_policy.unwrap_or_default())
        .map(|space_id| space_id.to_string())
        .map_err(runtime_error)
}

#[napi]
pub fn authority_mutate(store: &NativeStore, request: JsCreateMemoryRequest) -> Result<String> {
    let space_id = request.space_id.parse::<SpaceId>().map_err(to_napi_error)?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let memory_id = match request.idempotency_key.as_deref() {
        Some(idempotency_key) => runtime
            .create_memory_idempotent(
                space_id,
                request.document_key.as_deref(),
                request.mdx.as_bytes(),
                idempotency_key,
            )
            .map_err(runtime_error)?,
        None => runtime
            .create_memory(
                space_id,
                request.document_key.as_deref(),
                request.mdx.as_bytes(),
            )
            .map_err(runtime_error)?,
    };
    Ok(memory_id.to_string())
}

#[napi]
pub fn authority_revise(
    store: &NativeStore,
    request: JsReviseMemoryRequest,
) -> Result<JsMemoryMutation> {
    let memory_id = request
        .memory_id
        .parse::<MemoryId>()
        .map_err(to_napi_error)?;
    let expected_head = request
        .expected_head
        .parse::<RevisionId>()
        .map_err(to_napi_error)?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let mutation = runtime
        .revise_memory(memory_id, expected_head, request.mdx.as_bytes())
        .map_err(runtime_error)?;
    Ok(JsMemoryMutation {
        memory_id: mutation.memory_id.to_string(),
        space_id: mutation.space_id.to_string(),
        revision_id: mutation.revision_id.to_string(),
        authority_generation: mutation.generation.to_string(),
    })
}

#[napi]
pub fn export_memories(store: &NativeStore, scope: Vec<String>) -> Result<Vec<JsPortableMemory>> {
    let scope = scope
        .into_iter()
        .map(|space| space.parse::<SpaceId>().map_err(to_napi_error))
        .collect::<Result<Vec<_>>>()?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .export_memories(&scope)
        .map(|memories| memories.into_iter().map(JsPortableMemory::from).collect())
        .map_err(runtime_error)
}

#[napi]
pub fn import_portable(
    store: &NativeStore,
    request: JsPortableImportRequest,
) -> Result<JsPortableImportResult> {
    let request = request.try_into()?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .import_portable(request)
        .map(JsPortableImportResult::from)
        .map_err(runtime_error)
}

#[napi]
pub fn backup_create(
    store: &NativeStore,
    output_path: Option<String>,
    include_adaptive: bool,
) -> Result<JsBackupResult> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let output_path = output_path.map(std::path::PathBuf::from);
    let manifest = runtime
        .create_backup(output_path.as_deref(), include_adaptive)
        .map_err(runtime_error)?;
    JsBackupResult::try_from(manifest)
}

#[napi]
pub fn backup_restore(backup_path: String, target_path: String) -> Result<JsBackupResult> {
    let manifest =
        memoria_runtime::restore_store_backup(backup_path, &target_path).map_err(runtime_error)?;
    let mut result = JsBackupResult::try_from(manifest)?;
    result.path = target_path;
    Ok(result)
}

#[napi]
pub fn purge_plan(store: &NativeStore, memory_id: String) -> Result<JsPurgePlan> {
    let memory_id = memory_id.parse::<MemoryId>().map_err(to_napi_error)?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .plan_purge(memory_id)
        .map(JsPurgePlan::from)
        .map_err(runtime_error)
}

#[napi]
pub fn purge_execute(store: &NativeStore, plan_id: String) -> Result<JsPurgePlan> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .execute_purge(&plan_id)
        .map(JsPurgePlan::from)
        .map_err(runtime_error)
}

#[napi]
pub fn query_start(store: &NativeStore, request: JsQueryRequest) -> Result<JsQueryStep> {
    let core_request = QueryRequest::try_from(request)?;
    let query: MemoryQuery = core_request.into_core()?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .query_start(query)
        .map_err(runtime_error)
        .and_then(query_step_to_js)
}

#[napi]
pub fn query_continue(store: &NativeStore, operation_id: String) -> Result<JsQueryStep> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .query_continue(&operation_id)
        .map_err(runtime_error)
        .and_then(query_step_to_js)
}

#[napi]
pub fn feedback_submit(
    store: &NativeStore,
    request: JsFeedbackSubmission,
) -> Result<JsFeedbackCommit> {
    let submission =
        memoria_runtime::FeedbackSubmission::try_from(request).map_err(to_napi_error)?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .submit_feedback(submission)
        .map(JsFeedbackCommit::from)
        .map_err(runtime_error)
}

#[napi]
pub fn query_resume(
    store: &NativeStore,
    operation_id: String,
    result: JsProviderResult,
) -> Result<JsQueryStep> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .query_resume(&operation_id, provider_result_from_js(result)?)
        .map_err(runtime_error)
        .and_then(query_step_to_js)
}

#[napi]
pub fn cancel_operation(store: &NativeStore, operation_id: String) -> Result<()> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .cancel_operation(&operation_id)
        .map_err(runtime_error)
}

#[napi]
pub fn read_session_open(store: &NativeStore, request: JsQueryRequest) -> Result<String> {
    let query = QueryRequest::try_from(request)?.into_core()?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let session = runtime
        .open_read_session(query, Duration::from_secs(60))
        .map_err(runtime_error)?;
    let mut counter = store
        .next_session
        .lock()
        .map_err(|_| napi::Error::from_reason("session counter mutex poisoned"))?;
    *counter = counter.saturating_add(1);
    let id = format!("RS_{}", *counter);
    store
        .sessions
        .lock()
        .map_err(|_| napi::Error::from_reason("session mutex poisoned"))?
        .insert(id.clone(), session);
    Ok(id)
}

#[napi]
pub fn read_session_close(store: &NativeStore, session_id: String) -> Result<()> {
    store
        .sessions
        .lock()
        .map_err(|_| napi::Error::from_reason("session mutex poisoned"))?
        .remove(&session_id);
    Ok(())
}

#[napi]
pub fn provider_poll_work(store: &NativeStore) -> Result<Option<JsProviderWork>> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .provider_poll_work()
        .map(|work| work.map(JsProviderWork::from))
        .map_err(runtime_error)
}

#[napi]
pub fn provider_submit_result(store: &NativeStore, result: JsProviderResult) -> Result<()> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .provider_submit_result(provider_result_from_js(result)?)
        .map_err(runtime_error)
}
