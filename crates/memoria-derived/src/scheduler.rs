use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use memoria_types::AuthorityGeneration;

use crate::{
    DerivedCatalog, DerivedCompiler, DerivedError, DerivedManifest, DerivedStatus, LexicalDocument,
};

#[derive(Clone, Debug)]
struct BuildRequest {
    generation: AuthorityGeneration,
    documents: Vec<LexicalDocument>,
}

#[derive(Debug)]
struct SchedulerState {
    authority_generation: AuthorityGeneration,
    queued: Option<BuildRequest>,
    building: bool,
    paused: bool,
    shutdown: bool,
    last_error: Option<String>,
}

struct SchedulerInner {
    catalog: Mutex<DerivedCatalog>,
    compiler: DerivedCompiler,
    state: Mutex<SchedulerState>,
    wake: Condvar,
}

pub struct DerivedScheduler {
    inner: Arc<SchedulerInner>,
    worker: Option<JoinHandle<()>>,
}

impl DerivedScheduler {
    pub fn new(
        catalog: DerivedCatalog,
        compiler: DerivedCompiler,
        authority_generation: AuthorityGeneration,
    ) -> Self {
        let inner = Arc::new(SchedulerInner {
            catalog: Mutex::new(catalog),
            compiler,
            state: Mutex::new(SchedulerState {
                authority_generation,
                queued: None,
                building: false,
                paused: false,
                shutdown: false,
                last_error: None,
            }),
            wake: Condvar::new(),
        });
        let worker_inner = Arc::clone(&inner);
        let worker = thread::Builder::new()
            .name("memoria-derived-base".to_owned())
            .spawn(move || worker_loop(worker_inner))
            .expect("derived scheduler worker must start");
        Self {
            inner,
            worker: Some(worker),
        }
    }

    pub fn enqueue_base(
        &self,
        generation: AuthorityGeneration,
        documents: impl IntoIterator<Item = LexicalDocument>,
    ) -> Result<(), DerivedError> {
        let mut state = lock(&self.inner.state);
        if state.shutdown {
            return Err(DerivedError::SchedulerStopped);
        }
        if generation < state.authority_generation {
            return Ok(());
        }
        state.authority_generation = generation;
        state.queued = Some(BuildRequest {
            generation,
            documents: documents.into_iter().collect(),
        });
        state.last_error = None;
        self.inner.wake.notify_all();
        Ok(())
    }

    pub fn pause(&self) {
        let mut state = lock(&self.inner.state);
        state.paused = true;
    }

    pub fn resume(&self) {
        let mut state = lock(&self.inner.state);
        state.paused = false;
        self.inner.wake.notify_all();
    }

    #[must_use]
    pub fn status(&self) -> DerivedStatus {
        let state = lock(&self.inner.state);
        let catalog = lock(&self.inner.catalog);
        let manifest = catalog.serving_manifest().ok().flatten();
        DerivedStatus::from_manifest(
            state.authority_generation,
            manifest.as_ref(),
            state.queued.as_ref().map(|request| request.generation),
            state.building,
        )
    }

    pub fn wait_for_capabilities(
        &self,
        at_least: AuthorityGeneration,
        capability: &str,
        timeout: Duration,
    ) -> Result<DerivedManifest, DerivedError> {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let catalog = lock(&self.inner.catalog);
                if let Some(manifest) = catalog.serving_manifest()? {
                    let status = manifest.capability(capability);
                    if status.is_ready() && status.coverage() >= at_least {
                        return Ok(manifest);
                    }
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DerivedError::CapabilityTimeout {
                    capability: capability.to_owned(),
                    generation: at_least,
                });
            }
            let state = lock(&self.inner.state);
            let (_state_guard, result) = self
                .inner
                .wake
                .wait_timeout(state, remaining)
                .map_err(|_| DerivedError::SchedulerPoisoned)?;
            if result.timed_out() {
                return Err(DerivedError::CapabilityTimeout {
                    capability: capability.to_owned(),
                    generation: at_least,
                });
            }
        }
    }

    pub fn last_error(&self) -> Option<String> {
        lock(&self.inner.state).last_error.clone()
    }
}

impl Drop for DerivedScheduler {
    fn drop(&mut self) {
        {
            let mut state = lock(&self.inner.state);
            state.shutdown = true;
            state.queued = None;
            self.inner.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(inner: Arc<SchedulerInner>) {
    loop {
        let request = {
            let mut state = lock(&inner.state);
            while state.shutdown || state.paused || state.queued.is_none() {
                if state.shutdown {
                    return;
                }
                state = match inner.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            state.building = true;
            state.queued.take()
        };
        let Some(request) = request else {
            continue;
        };
        let result = {
            let mut catalog = lock(&inner.catalog);
            inner
                .compiler
                .compile_base(&mut catalog, request.generation, request.documents)
        };
        let mut state = lock(&inner.state);
        state.building = false;
        if let Err(error) = result {
            state.last_error = Some(error.to_string());
        }
        inner.wake.notify_all();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
