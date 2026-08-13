pub mod convert;
mod error;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use memoria_query::{MemoryQuery, ReadSession};
use memoria_runtime::{MemoriaRuntime, ProviderWorkResult};
use memoria_types::SpaceId;
use napi::bindgen_prelude::Result;
use napi_derive::napi;

use crate::convert::{
    JsCreateMemoryRequest, JsProviderResult, JsQueryRequest, JsQueryResponse, JsStatus,
    QueryRequest,
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
pub fn authority_mutate(store: &NativeStore, request: JsCreateMemoryRequest) -> Result<String> {
    let space_id = request.space_id.parse::<SpaceId>().map_err(to_napi_error)?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let memory_id = runtime
        .create_memory(
            space_id,
            request.document_key.as_deref(),
            request.mdx.as_bytes(),
        )
        .map_err(runtime_error)?;
    Ok(memory_id.to_string())
}

#[napi]
pub fn query_start(store: &NativeStore, request: JsQueryRequest) -> Result<JsQueryResponse> {
    let core_request = QueryRequest::try_from(request)?;
    let query: MemoryQuery = core_request.into_core()?;
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    let response = runtime.query(query).map_err(runtime_error)?;
    Ok(JsQueryResponse {
        result_count: u32::try_from(response.results.len()).map_err(to_napi_error)?,
        authority_generation: response.snapshot.authority_generation.to_string(),
        degraded: response.execution.degraded,
    })
}

#[napi]
pub fn query_resume(_store: &NativeStore, _operation_id: String) -> Result<JsQueryResponse> {
    Err(napi::Error::from_reason(
        "query operation is already complete or unavailable",
    ))
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
pub fn provider_poll_work(_store: &NativeStore) -> Result<Option<String>> {
    Ok(None)
}

#[napi]
pub fn provider_submit_result(store: &NativeStore, result: JsProviderResult) -> Result<()> {
    let mut runtime = store.runtime()?;
    let runtime = runtime
        .as_mut()
        .ok_or_else(|| napi::Error::from_reason("store is closed"))?;
    runtime
        .provider_submit_result(ProviderWorkResult {
            work_id: result.work_id,
            accepted: result.accepted,
        })
        .map_err(runtime_error)
}

#[napi]
pub fn cancel_operation(_store: &NativeStore, _operation_id: String) -> Result<()> {
    Ok(())
}
