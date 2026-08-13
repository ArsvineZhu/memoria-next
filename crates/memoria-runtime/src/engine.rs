use std::string::FromUtf8Error;

use memoria_authority::{AuthorityDb, MemoryLifecycle, SourceCas, StoreLayout, StoreWriterLock};
use memoria_derived::{
    DerivedCatalog, DerivedCompiler, EntityObservationBuilder, ExplicitTagBuilder, LexicalDocument,
};
use memoria_mdx::compile_ir;
use memoria_query::{
    ExactIndex, ExactRecord, LexicalCandidate, LexicalCandidateIndex, MemoryQuery, QueryCompiler,
    ReadSession, RetrievalResponse, build_response, execute_exact, execute_lexical,
};
use memoria_types::{AuthorityGeneration, MemoriaError, MemoryId, SpaceId};
use thiserror::Error;

use crate::provider::{NeedWork, ProviderWorkResult};
use crate::status::RuntimeStatus;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime is closed")]
    Closed,

    #[error("authority error: {0}")]
    Authority(#[from] MemoriaError),

    #[error("authority database error: {message}")]
    AuthorityDatabase { message: String },

    #[error("Derived error: {0}")]
    Derived(#[from] memoria_derived::DerivedError),

    #[error("MDX error: {0}")]
    Mdx(#[from] memoria_mdx::MdxError),

    #[error("query error: {0}")]
    Query(#[from] memoria_query::QueryError),

    #[error("consolidation error: {0}")]
    Consolidation(#[from] memoria_query::ConsolidationError),

    #[error("source is not valid UTF-8: {0}")]
    Utf8(#[from] FromUtf8Error),

    #[error("provider result `{work_id}` was not expected")]
    UnexpectedProviderWork { work_id: String },
}

pub struct MemoriaRuntime {
    _writer_lock: StoreWriterLock,
    layout: StoreLayout,
    authority: AuthorityDb,
    cas: SourceCas,
    derived: DerivedCatalog,
    compiler: DerivedCompiler,
    closed: bool,
    last_error: Option<String>,
}

impl MemoriaRuntime {
    pub fn open(data_dir: impl AsRef<std::path::Path>) -> Result<Self, RuntimeError> {
        let layout = StoreLayout::create(data_dir)?;
        let writer_lock = StoreWriterLock::acquire(layout.store_dir())?;
        let authority = AuthorityDb::open(layout.authority_database()).map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let derived = DerivedCatalog::open(layout.derived_dir().join("catalog.sqlite"))?;
        Ok(Self {
            _writer_lock: writer_lock,
            cas: SourceCas::new(&layout),
            layout,
            authority,
            derived,
            compiler: DerivedCompiler::default(),
            closed: false,
            last_error: None,
        })
    }

    pub fn close(&mut self) -> Result<(), RuntimeError> {
        self.closed = true;
        Ok(())
    }

    pub fn create_space(&mut self, space_key: impl AsRef<str>) -> Result<SpaceId, RuntimeError> {
        self.ensure_open()?;
        Ok(self
            .authority
            .create_space(space_key)?
            .into_value()
            .space_id)
    }

    pub fn create_memory(
        &mut self,
        space_id: SpaceId,
        document_key: Option<&str>,
        source: &[u8],
    ) -> Result<MemoryId, RuntimeError> {
        self.ensure_open()?;
        let result = self
            .authority
            .create_memory(&self.cas, space_id, document_key, source)?;
        let memory_id = result.value().memory_id;
        self.rebuild_base_for_space(space_id, result.generation());
        Ok(memory_id)
    }

    pub fn query(&mut self, query: MemoryQuery) -> Result<RetrievalResponse, RuntimeError> {
        self.ensure_open()?;
        query.validate()?;
        let generation = self.authority.current_generation().map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let records = self.records_for_query(&query, generation)?;
        let manifest = self
            .derived
            .serving_manifest()?
            .ok_or(RuntimeError::AuthorityDatabase {
                message: "no Derived Manifest is serving".to_owned(),
            })?;
        let compiled = QueryCompiler::new(generation, Some(manifest)).compile(query)?;
        let candidates = if compiled.query.cue.text.is_empty() {
            execute_exact(&compiled, &ExactIndex::new(records)).results
        } else {
            let lexical = records
                .iter()
                .map(|record| LexicalCandidate {
                    target: record.target,
                    score: text_score(record, &compiled.query.cue.text),
                    text: record.text.clone(),
                    entity_refs: record.entity_refs.clone(),
                    tags: record.tags.clone(),
                    current: record.current,
                    retired: record.retired,
                })
                .collect();
            execute_lexical(&compiled, &LexicalCandidateIndex::new(lexical)).results
        };
        Ok(build_response("runtime-query", &compiled, candidates)?)
    }

    pub fn open_read_session(
        &mut self,
        query: MemoryQuery,
        ttl: std::time::Duration,
    ) -> Result<ReadSession, RuntimeError> {
        self.ensure_open()?;
        query.validate()?;
        let generation = self.authority.current_generation().map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })?;
        let manifest = self.derived.serving_manifest().ok().flatten().ok_or(
            RuntimeError::AuthorityDatabase {
                message: "no Derived Manifest is serving".to_owned(),
            },
        )?;
        let compiled = QueryCompiler::new(generation, Some(manifest)).compile(query)?;
        ReadSession::open(&mut self.derived, &compiled, ttl).map_err(|error| {
            RuntimeError::AuthorityDatabase {
                message: error.to_string(),
            }
        })
    }

    #[must_use]
    pub fn status(&self) -> RuntimeStatus {
        let authority_generation = self
            .authority
            .current_generation()
            .unwrap_or_else(|_| AuthorityGeneration::initial());
        let (base_coverage, last_error) = self
            .derived
            .serving_manifest()
            .ok()
            .flatten()
            .map(|manifest| {
                (
                    manifest.capability("base-search").coverage(),
                    self.last_error.clone(),
                )
            })
            .unwrap_or((AuthorityGeneration::initial(), self.last_error.clone()));
        RuntimeStatus {
            authority_generation,
            base_coverage,
            semantic_coverage: AuthorityGeneration::initial(),
            active_read_leases: 0,
            closed: self.closed,
            last_error,
        }
    }

    pub fn provider_poll_work(&mut self) -> Result<Option<NeedWork>, RuntimeError> {
        self.ensure_open()?;
        Ok(None)
    }

    pub fn provider_submit_result(
        &mut self,
        result: ProviderWorkResult,
    ) -> Result<(), RuntimeError> {
        self.ensure_open()?;
        if result.accepted {
            return Ok(());
        }
        Err(RuntimeError::UnexpectedProviderWork {
            work_id: result.work_id,
        })
    }

    fn rebuild_base_for_space(&mut self, space_id: SpaceId, generation: AuthorityGeneration) {
        if let Err(error) = self.try_rebuild_base_for_space(space_id, generation) {
            self.last_error = Some(error.to_string());
        }
    }

    fn try_rebuild_base_for_space(
        &mut self,
        space_id: SpaceId,
        generation: AuthorityGeneration,
    ) -> Result<(), RuntimeError> {
        let reads = self
            .authority
            .list_memories_at(&self.cas, space_id, generation)?;
        let mut documents = Vec::with_capacity(reads.len());
        for read in &reads {
            let source = String::from_utf8(read.source.clone())?;
            documents.push(LexicalDocument::new(
                space_id,
                read.memory.memory_id,
                read.memory.head_revision_id,
                compile_ir(&source)?,
            ));
        }
        self.compiler
            .compile_base(&mut self.derived, generation, documents)?;
        Ok(())
    }

    fn records_for_query(
        &mut self,
        query: &MemoryQuery,
        generation: AuthorityGeneration,
    ) -> Result<Vec<ExactRecord>, RuntimeError> {
        let mut records = Vec::new();
        for space_id in &query.scope.spaces {
            let reads = self
                .authority
                .list_memories_at(&self.cas, *space_id, generation)?;
            if self.derived.serving_manifest()?.is_none_or(|manifest| {
                manifest.authority_generation() < generation
                    || !manifest.capability("base-search").is_ready()
            }) {
                self.try_rebuild_base_for_space(*space_id, generation)?;
            }
            for read in reads {
                let source = String::from_utf8(read.source)?;
                let ir = compile_ir(&source)?;
                let target = memoria_derived::ProjectionTarget {
                    space_id: Some(*space_id),
                    memory_id: Some(read.memory.memory_id),
                    revision_id: Some(read.memory.head_revision_id),
                };
                let entities = EntityObservationBuilder::build_for(&ir, target)?
                    .observations()
                    .iter()
                    .map(|observation| {
                        memoria_query::EntityRef::new(observation.entity_ref.as_str())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let tags = ExplicitTagBuilder::build_for(&ir, target)?
                    .memberships()
                    .iter()
                    .map(|membership| membership.value.clone())
                    .collect();
                let node_ids = ir
                    .nodes()
                    .filter_map(|node| node.id().map(ToString::to_string))
                    .collect();
                records.push(
                    ExactRecord::new(
                        *space_id,
                        read.memory.memory_id,
                        read.memory.head_revision_id,
                        source,
                    )
                    .with_entities(entities)
                    .with_tags(tags)
                    .with_node_ids(node_ids)
                    .with_authority_generation(read.memory.generation)
                    .with_retired(read.memory.lifecycle == MemoryLifecycle::Retired),
                );
            }
        }
        Ok(records)
    }

    fn ensure_open(&self) -> Result<(), RuntimeError> {
        if self.closed {
            Err(RuntimeError::Closed)
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn data_dir(&self) -> &std::path::Path {
        self.layout.store_dir()
    }
}

fn text_score(record: &ExactRecord, cues: &[String]) -> f32 {
    cues.iter()
        .filter(|cue| {
            record
                .text
                .to_ascii_lowercase()
                .contains(&cue.to_ascii_lowercase())
        })
        .count() as f32
}
