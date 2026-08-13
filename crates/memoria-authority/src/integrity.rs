use std::collections::{HashMap, HashSet};

use memoria_types::{MemoriaError, MemoryId, RevisionId, SourceBlobHash, SpaceId};
use rusqlite::{Connection, params};

use crate::cas::SourceCas;
use crate::db::AuthorityDb;
use crate::model::database_error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntegrityIssue {
    pub code: String,
    pub message: String,
}

impl IntegrityIssue {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntegrityReport {
    issues: Vec<IntegrityIssue>,
}

impl IntegrityReport {
    #[must_use]
    pub fn issues(&self) -> &[IntegrityIssue] {
        &self.issues
    }

    #[must_use]
    pub fn has_authority_corruption(&self) -> bool {
        !self.issues.is_empty()
    }

    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    fn push(&mut self, code: impl Into<String>, message: impl Into<String>) {
        self.issues.push(IntegrityIssue::new(code, message));
    }
}

#[derive(Clone, Debug)]
struct SpaceStateRow {
    space_id: SpaceId,
    space_key: String,
    valid_from: i64,
    valid_to: Option<i64>,
}

#[derive(Clone, Debug)]
struct MemoryStateRow {
    memory_id: MemoryId,
    space_id: SpaceId,
    document_key: Option<String>,
    head_revision_id: Option<RevisionId>,
    valid_from: i64,
    valid_to: Option<i64>,
}

#[derive(Clone, Debug)]
struct RevisionNode {
    revision_id: RevisionId,
    memory_id: MemoryId,
    source_blob_hash: SourceBlobHash,
    committed_generation: i64,
    parents: Vec<RevisionId>,
}

impl AuthorityDb {
    pub fn verify_fast(&self, cas: &SourceCas) -> Result<IntegrityReport, MemoriaError> {
        self.verify_integrity(cas, false)
    }

    pub fn verify_full(&self, cas: &SourceCas) -> Result<IntegrityReport, MemoriaError> {
        self.verify_integrity(cas, true)
    }

    fn verify_integrity(
        &self,
        cas: &SourceCas,
        full: bool,
    ) -> Result<IntegrityReport, MemoriaError> {
        let (snapshot, mut report) = self
            .read(|connection| {
                let snapshot = load_snapshot(connection)?;
                let mut report = IntegrityReport::default();
                check_sqlite_integrity(connection, &mut report)?;
                Ok((snapshot, report))
            })
            .map_err(database_error)?;
        check_generation_and_intervals(&snapshot, &mut report);
        check_space_states(&snapshot, &mut report);
        check_memory_states(&snapshot, &mut report);
        check_revision_references(&snapshot, &mut report);

        if full {
            check_revision_dag(&snapshot.revisions, &mut report);
            check_source_objects(&snapshot.revisions, cas, &mut report);
        } else {
            check_current_source_objects(&snapshot, cas, &mut report);
        }

        Ok(report)
    }
}

struct AuthoritySnapshot {
    current_generation: i64,
    spaces: Vec<(SpaceId, i64)>,
    space_states: Vec<SpaceStateRow>,
    memories: Vec<(MemoryId, i64)>,
    memory_states: Vec<MemoryStateRow>,
    revisions: Vec<RevisionNode>,
}

fn load_snapshot(connection: &Connection) -> rusqlite::Result<AuthoritySnapshot> {
    let current_generation = connection.query_row(
        "SELECT generation FROM authority_generation WHERE id = 1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    let spaces = load_spaces(connection)?;
    let space_states = load_space_states(connection)?;
    let memories = load_memories(connection)?;
    let memory_states = load_memory_states(connection)?;
    let revisions = load_revisions(connection)?;
    Ok(AuthoritySnapshot {
        current_generation,
        spaces,
        space_states,
        memories,
        memory_states,
        revisions,
    })
}

fn check_sqlite_integrity(
    connection: &Connection,
    report: &mut IntegrityReport,
) -> rusqlite::Result<()> {
    let integrity =
        connection.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;
    if integrity != "ok" {
        report.push("SQLITE_INTEGRITY", integrity);
    }

    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let violations = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (table, rowid, parent) in violations {
        report.push(
            "FOREIGN_KEY",
            format!("{table} row {rowid} references missing {parent}"),
        );
    }
    Ok(())
}

fn check_generation_and_intervals(snapshot: &AuthoritySnapshot, report: &mut IntegrityReport) {
    if snapshot.current_generation < 0 {
        report.push("GENERATION_NEGATIVE", "authority_generation is negative");
    }
    for (kind, id, generation) in snapshot
        .spaces
        .iter()
        .map(|(id, generation)| ("Space", id.to_string(), *generation))
        .chain(
            snapshot
                .memories
                .iter()
                .map(|(id, generation)| ("Memory", id.to_string(), *generation)),
        )
    {
        if generation < 0 || generation > snapshot.current_generation {
            report.push(
                "IDENTITY_GENERATION",
                format!("{kind} {id} has invalid creation generation {generation}"),
            );
        }
    }
    check_intervals(
        snapshot.space_states.iter().map(|state| {
            (
                "space",
                state.space_id.to_string(),
                state.valid_from,
                state.valid_to,
            )
        }),
        snapshot.current_generation,
        report,
    );
    check_intervals(
        snapshot.memory_states.iter().map(|state| {
            (
                "memory",
                state.memory_id.to_string(),
                state.valid_from,
                state.valid_to,
            )
        }),
        snapshot.current_generation,
        report,
    );
    check_interval_overlaps(
        "space",
        snapshot
            .space_states
            .iter()
            .map(|state| (state.space_id, state.valid_from, state.valid_to)),
        report,
    );
    check_interval_overlaps(
        "memory",
        snapshot
            .memory_states
            .iter()
            .map(|state| (state.memory_id, state.valid_from, state.valid_to)),
        report,
    );
}

fn check_intervals<'a, I>(intervals: I, current_generation: i64, report: &mut IntegrityReport)
where
    I: IntoIterator<Item = (&'a str, String, i64, Option<i64>)>,
{
    for (kind, id, valid_from, valid_to) in intervals {
        if valid_from < 0 || valid_from > current_generation {
            report.push(
                "STATE_GENERATION",
                format!("{kind} {id} has invalid valid_from_generation {valid_from}"),
            );
        }
        if let Some(valid_to) = valid_to
            && (valid_to <= valid_from || valid_to > current_generation)
        {
            report.push(
                "STATE_INTERVAL",
                format!("{kind} {id} has invalid interval {valid_from}..{valid_to}"),
            );
        }
    }
}

fn check_interval_overlaps<I, T>(kind: &str, intervals: I, report: &mut IntegrityReport)
where
    I: IntoIterator<Item = (T, i64, Option<i64>)>,
    T: Copy + Eq + std::fmt::Display + std::hash::Hash,
{
    let mut grouped = HashMap::<T, Vec<(i64, Option<i64>)>>::new();
    for (id, valid_from, valid_to) in intervals {
        grouped.entry(id).or_default().push((valid_from, valid_to));
    }
    for (id, mut ranges) in grouped {
        ranges.sort_by_key(|(valid_from, _)| *valid_from);
        for pair in ranges.windows(2) {
            let previous_end = pair[0].1.unwrap_or(i64::MAX);
            if pair[1].0 < previous_end {
                report.push(
                    "STATE_OVERLAP",
                    format!("{kind} {id} has overlapping state intervals"),
                );
                break;
            }
        }
    }
}

fn check_space_states(snapshot: &AuthoritySnapshot, report: &mut IntegrityReport) {
    let space_ids = snapshot
        .spaces
        .iter()
        .map(|(space_id, _)| *space_id)
        .collect::<HashSet<_>>();
    let mut state_counts = HashMap::<SpaceId, usize>::new();
    let mut current_keys = HashMap::<String, SpaceId>::new();
    for state in &snapshot.space_states {
        if !space_ids.contains(&state.space_id) {
            report.push(
                "SPACE_IDENTITY",
                format!("space state references unknown Space {}", state.space_id),
            );
        }
        *state_counts.entry(state.space_id).or_default() += 1;
        if state.valid_to.is_none()
            && let Some(previous) = current_keys.insert(state.space_key.clone(), state.space_id)
            && previous != state.space_id
        {
            report.push(
                "SPACE_KEY_DUPLICATE",
                format!(
                    "current Space key `{}` is used by multiple Spaces",
                    state.space_key
                ),
            );
        }
    }
    for (space_id, created_generation) in &snapshot.spaces {
        let states = snapshot
            .space_states
            .iter()
            .filter(|state| state.space_id == *space_id)
            .collect::<Vec<_>>();
        if state_counts.get(space_id).copied().unwrap_or(0) == 0 {
            report.push(
                "SPACE_STATE_MISSING",
                format!("Space {space_id} has no state history"),
            );
        }
        if states
            .iter()
            .filter(|state| state.valid_to.is_none())
            .count()
            != 1
        {
            report.push(
                "SPACE_CURRENT_STATE",
                format!("Space {space_id} does not have exactly one current state"),
            );
        }
        if states.iter().map(|state| state.valid_from).min() != Some(*created_generation) {
            report.push(
                "SPACE_CREATION_GENERATION",
                format!("Space {space_id} state history does not start at creation generation"),
            );
        }
    }
}

fn check_memory_states(snapshot: &AuthoritySnapshot, report: &mut IntegrityReport) {
    let space_ids = snapshot
        .spaces
        .iter()
        .map(|(space_id, _)| *space_id)
        .collect::<HashSet<_>>();
    let memory_ids = snapshot
        .memories
        .iter()
        .map(|(memory_id, _)| *memory_id)
        .collect::<HashSet<_>>();
    let mut current_document_keys = HashMap::<(SpaceId, String), MemoryId>::new();
    for state in &snapshot.memory_states {
        if !memory_ids.contains(&state.memory_id) {
            report.push(
                "MEMORY_IDENTITY",
                format!("memory state references unknown Memory {}", state.memory_id),
            );
        }
        if !space_ids.contains(&state.space_id) {
            report.push(
                "MEMORY_SPACE",
                format!(
                    "Memory {} references unknown Space {}",
                    state.memory_id, state.space_id
                ),
            );
        }
        if state.valid_to.is_none()
            && let Some(document_key) = &state.document_key
            && let Some(previous) = current_document_keys
                .insert((state.space_id, document_key.clone()), state.memory_id)
            && previous != state.memory_id
        {
            report.push(
                "DOCUMENT_KEY_DUPLICATE",
                format!(
                    "current document key `{document_key}` is duplicated in Space {}",
                    state.space_id
                ),
            );
        }
    }
    for (memory_id, created_generation) in &snapshot.memories {
        let states = snapshot
            .memory_states
            .iter()
            .filter(|state| state.memory_id == *memory_id)
            .collect::<Vec<_>>();
        if states.is_empty() {
            report.push(
                "MEMORY_STATE_MISSING",
                format!("Memory {memory_id} has no state history"),
            );
        }
        if states
            .iter()
            .filter(|state| state.valid_to.is_none())
            .count()
            != 1
        {
            report.push(
                "MEMORY_CURRENT_STATE",
                format!("Memory {memory_id} does not have exactly one current state"),
            );
        }
        if states.iter().map(|state| state.valid_from).min() != Some(*created_generation) {
            report.push(
                "MEMORY_CREATION_GENERATION",
                format!("Memory {memory_id} state history does not start at creation generation"),
            );
        }
    }
}

fn check_revision_references(snapshot: &AuthoritySnapshot, report: &mut IntegrityReport) {
    let revision_map = snapshot
        .revisions
        .iter()
        .map(|revision| (revision.revision_id, revision))
        .collect::<HashMap<_, _>>();
    let memory_map = snapshot
        .memories
        .iter()
        .map(|(memory_id, created_generation)| (*memory_id, *created_generation))
        .collect::<HashMap<_, _>>();
    for revision in &snapshot.revisions {
        if revision.committed_generation < 0
            || revision.committed_generation > snapshot.current_generation
        {
            report.push(
                "REVISION_GENERATION",
                format!(
                    "revision {} has invalid committed generation {}",
                    revision.revision_id, revision.committed_generation
                ),
            );
        }
        if !memory_map.contains_key(&revision.memory_id) {
            report.push(
                "REVISION_MEMORY",
                format!(
                    "revision {} references unknown Memory {}",
                    revision.revision_id, revision.memory_id
                ),
            );
        }
        for parent in &revision.parents {
            match revision_map.get(parent) {
                None => report.push(
                    "REVISION_PARENT_MISSING",
                    format!(
                        "revision {} references missing parent {}",
                        revision.revision_id, parent
                    ),
                ),
                Some(parent_revision) if parent_revision.memory_id != revision.memory_id => {
                    report.push(
                        "REVISION_PARENT_MEMORY",
                        format!(
                            "revision {} parent {} belongs to another Memory",
                            revision.revision_id, parent
                        ),
                    );
                }
                Some(_) => {}
            }
        }
    }
    for state in &snapshot.memory_states {
        let Some(head_revision_id) = state.head_revision_id else {
            report.push(
                "HEAD_MISSING",
                format!("Memory {} state has no HEAD revision", state.memory_id),
            );
            continue;
        };
        match revision_map.get(&head_revision_id) {
            None => report.push(
                "HEAD_MISSING",
                format!(
                    "Memory {} references missing HEAD {}",
                    state.memory_id, head_revision_id
                ),
            ),
            Some(revision) if revision.memory_id != state.memory_id => report.push(
                "HEAD_OWNERSHIP",
                format!(
                    "Memory {} HEAD {} belongs to another Memory",
                    state.memory_id, head_revision_id
                ),
            ),
            Some(revision) if revision.committed_generation > state.valid_from => report.push(
                "HEAD_GENERATION",
                format!(
                    "Memory {} state points to a future HEAD {}",
                    state.memory_id, head_revision_id
                ),
            ),
            Some(_) => {}
        }
    }
}

fn check_revision_dag(revisions: &[RevisionNode], report: &mut IntegrityReport) {
    let graph = revisions
        .iter()
        .map(|revision| (revision.revision_id, revision.parents.clone()))
        .collect::<HashMap<_, _>>();
    let mut colors = HashMap::<RevisionId, VisitColor>::new();
    for revision in revisions {
        if !visit_revision(revision.revision_id, &graph, &mut colors) {
            report.push(
                "REVISION_CYCLE",
                format!(
                    "revision graph contains a cycle at {}",
                    revision.revision_id
                ),
            );
            break;
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitColor {
    Visiting,
    Visited,
}

fn visit_revision(
    revision_id: RevisionId,
    graph: &HashMap<RevisionId, Vec<RevisionId>>,
    colors: &mut HashMap<RevisionId, VisitColor>,
) -> bool {
    match colors.get(&revision_id) {
        Some(VisitColor::Visiting) => return false,
        Some(VisitColor::Visited) => return true,
        None => {}
    }
    colors.insert(revision_id, VisitColor::Visiting);
    if let Some(parents) = graph.get(&revision_id) {
        for parent in parents {
            if !visit_revision(*parent, graph, colors) {
                return false;
            }
        }
    }
    colors.insert(revision_id, VisitColor::Visited);
    true
}

fn check_current_source_objects(
    snapshot: &AuthoritySnapshot,
    cas: &SourceCas,
    report: &mut IntegrityReport,
) {
    let revision_map = snapshot
        .revisions
        .iter()
        .map(|revision| (revision.revision_id, revision))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    for state in snapshot
        .memory_states
        .iter()
        .filter(|state| state.valid_to.is_none())
    {
        let Some(head_revision_id) = state.head_revision_id else {
            continue;
        };
        let Some(revision) = revision_map.get(&head_revision_id) else {
            continue;
        };
        if seen.insert(revision.source_blob_hash) {
            check_source_object(revision, cas, report);
        }
    }
}

fn check_source_objects(revisions: &[RevisionNode], cas: &SourceCas, report: &mut IntegrityReport) {
    let mut seen = HashSet::new();
    for revision in revisions {
        if seen.insert(revision.source_blob_hash) {
            check_source_object(revision, cas, report);
        }
    }
}

fn check_source_object(revision: &RevisionNode, cas: &SourceCas, report: &mut IntegrityReport) {
    match cas.get(revision.source_blob_hash) {
        Err(error) => report.push(
            "SOURCE_BLOB_MISSING",
            format!(
                "revision {} source {} cannot be read: {error}",
                revision.revision_id, revision.source_blob_hash
            ),
        ),
        Ok(source) if SourceBlobHash::from_bytes(&source) != revision.source_blob_hash => report
            .push(
                "SOURCE_BLOB_HASH",
                format!(
                    "revision {} source object {} does not match its content hash",
                    revision.revision_id, revision.source_blob_hash
                ),
            ),
        Ok(_) => {}
    }
}

fn load_spaces(connection: &Connection) -> rusqlite::Result<Vec<(SpaceId, i64)>> {
    let mut statement = connection.prepare("SELECT space_id, created_generation FROM spaces")?;
    statement
        .query_map([], |row| {
            Ok((
                SpaceId::from_bytes(parse_fixed_bytes(row.get(0)?, "space id")?),
                row.get(1)?,
            ))
        })?
        .collect()
}

fn load_space_states(connection: &Connection) -> rusqlite::Result<Vec<SpaceStateRow>> {
    let mut statement = connection.prepare(
        "SELECT space_id, space_key, valid_from_generation, valid_to_generation
         FROM space_state_history",
    )?;
    statement
        .query_map([], |row| {
            Ok(SpaceStateRow {
                space_id: SpaceId::from_bytes(parse_fixed_bytes(row.get(0)?, "space id")?),
                space_key: row.get(1)?,
                valid_from: row.get(2)?,
                valid_to: row.get(3)?,
            })
        })?
        .collect()
}

fn load_memories(connection: &Connection) -> rusqlite::Result<Vec<(MemoryId, i64)>> {
    let mut statement = connection.prepare("SELECT memory_id, created_generation FROM memories")?;
    statement
        .query_map([], |row| {
            Ok((
                MemoryId::from_bytes(parse_fixed_bytes(row.get(0)?, "memory id")?),
                row.get(1)?,
            ))
        })?
        .collect()
}

fn load_memory_states(connection: &Connection) -> rusqlite::Result<Vec<MemoryStateRow>> {
    let mut statement = connection.prepare(
        "SELECT memory_id, space_id, document_key, head_revision_id,
                valid_from_generation, valid_to_generation
         FROM memory_state_history",
    )?;
    statement
        .query_map([], |row| {
            Ok(MemoryStateRow {
                memory_id: MemoryId::from_bytes(parse_fixed_bytes(row.get(0)?, "memory id")?),
                space_id: SpaceId::from_bytes(parse_fixed_bytes(row.get(1)?, "space id")?),
                document_key: row.get(2)?,
                head_revision_id: row
                    .get::<_, Option<Vec<u8>>>(3)?
                    .map(|bytes| parse_fixed_bytes(bytes, "revision id"))
                    .transpose()?
                    .map(RevisionId::from_bytes),
                valid_from: row.get(4)?,
                valid_to: row.get(5)?,
            })
        })?
        .collect()
}

fn load_revisions(connection: &Connection) -> rusqlite::Result<Vec<RevisionNode>> {
    let mut statement = connection.prepare(
        "SELECT revision_id, memory_id, source_blob_hash, committed_generation
         FROM revisions",
    )?;
    let mut revisions = statement
        .query_map([], |row| {
            let source_blob_hash =
                row.get::<_, String>(2)?
                    .parse::<SourceBlobHash>()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
            Ok(RevisionNode {
                revision_id: RevisionId::from_bytes(parse_fixed_bytes(row.get(0)?, "revision id")?),
                memory_id: MemoryId::from_bytes(parse_fixed_bytes(row.get(1)?, "memory id")?),
                source_blob_hash,
                committed_generation: row.get(3)?,
                parents: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut parent_statement = connection.prepare(
        "SELECT parent_revision_id
         FROM revision_parents
         WHERE revision_id = ?1
         ORDER BY parent_order",
    )?;
    for revision in &mut revisions {
        revision.parents = parent_statement
            .query_map(params![revision.revision_id.as_bytes().as_slice()], |row| {
                Ok(RevisionId::from_bytes(parse_fixed_bytes(
                    row.get::<_, Vec<u8>>(0)?,
                    "revision id",
                )?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(revisions)
}

fn parse_fixed_bytes<const N: usize>(
    bytes: Vec<u8>,
    kind: &'static str,
) -> rusqlite::Result<[u8; N]> {
    bytes.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{kind} has an invalid byte length"),
            )),
        )
    })
}
