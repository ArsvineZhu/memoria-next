use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use memoria_types::{AuthorityGeneration, MemoryId, SpaceId};
use sha2::{Digest, Sha256};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Query, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, STORED, STRING, Schema, TEXT};
use tantivy::{Index, IndexReader, Term, doc};

use crate::DerivedError;
use crate::projection::lexical::{LexicalDocument, LexicalHit, lexical_text, stored_id};

pub const LEXICAL_ARTIFACT_VERSION: u32 = 1;

static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LexicalArtifactHash([u8; 32]);

impl LexicalArtifactHash {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn as_hex(&self) -> String {
        hex_lower(&self.0)
    }
}

impl fmt::Display for LexicalArtifactHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_hex())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexicalArtifactV1 {
    hash: LexicalArtifactHash,
    authority_generation: AuthorityGeneration,
    document_count: usize,
    object_path: String,
}

impl LexicalArtifactV1 {
    pub fn build(
        derived_dir: impl AsRef<Path>,
        authority_generation: AuthorityGeneration,
        documents: &[LexicalDocument],
    ) -> Result<Self, DerivedError> {
        let derived_dir = derived_dir.as_ref();
        let objects_dir = derived_dir.join("objects").join("lexical");
        fs::create_dir_all(&objects_dir)?;
        let staging = derived_dir.join(format!(
            "lexical-build-{}-{}",
            MemoryId::new(),
            NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed)
        ));
        if staging.exists() {
            return Err(DerivedError::InvalidProjectionValue {
                value: format!("lexical staging path already exists: {}", staging.display()),
            });
        }

        let build_result = (|| {
            fs::create_dir_all(&staging)?;
            let (schema, text_field, space_field, memory_field, revision_field) = schema_v1();
            let derived_unit_field = schema
                .get_field("derived_unit_id")
                .map_err(DerivedError::from)?;
            let resolution_field = schema.get_field("resolution").map_err(DerivedError::from)?;
            let index = Index::create_in_dir(&staging, schema)?;
            let mut writer = index.writer(50_000_000)?;
            for document in documents {
                let memory_id = document.memory_id().to_string();
                let revision_id = document.revision_id().to_string();
                let derived_unit_id = document.derived_unit_id();
                writer.add_document(doc!(
                    text_field => lexical_text(document.ir()),
                    space_field => document.space_id().to_string(),
                    memory_field => memory_id.clone(),
                    revision_field => revision_id,
                    derived_unit_field => derived_unit_id,
                    resolution_field => document.resolution(),
                ))?;
            }
            writer.commit()?;
            drop(writer);
            let reader = index.reader()?;
            reader.reload()?;
            drop(reader);
            drop(index);
            sync_tree(&staging)?;

            let hash = LexicalArtifactHash::from_bytes(hash_tree(&staging)?);
            let object_path = format!("objects/lexical/{hash}");
            let target = derived_dir.join(&object_path);
            if target.exists() {
                if !target.is_dir() || hash_tree(&target)? != *hash.as_bytes() {
                    return Err(DerivedError::InvalidProjectionValue {
                        value: format!(
                            "lexical artifact object hash mismatch: {}",
                            target.display()
                        ),
                    });
                }
                fs::remove_dir_all(&staging)?;
            } else {
                fs::rename(&staging, &target)?;
                sync_directory(target.parent().ok_or_else(|| {
                    DerivedError::InvalidProjectionValue {
                        value: "lexical artifact target has no parent".to_owned(),
                    }
                })?)?;
            }
            Ok(Self {
                hash,
                authority_generation,
                document_count: documents.len(),
                object_path,
            })
        })();

        if build_result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        build_result
    }

    #[must_use]
    pub const fn hash(&self) -> LexicalArtifactHash {
        self.hash
    }

    #[must_use]
    pub const fn authority_generation(&self) -> AuthorityGeneration {
        self.authority_generation
    }

    #[must_use]
    pub const fn document_count(&self) -> usize {
        self.document_count
    }

    #[must_use]
    pub fn object_path(&self) -> &str {
        &self.object_path
    }

    pub fn open(
        &self,
        derived_dir: impl AsRef<Path>,
    ) -> Result<LexicalArtifactHandle, DerivedError> {
        LexicalArtifactHandle::open(derived_dir.as_ref().join(&self.object_path))
    }
}

pub struct LexicalArtifactHandle {
    index: Index,
    reader: IndexReader,
    text_field: tantivy::schema::Field,
    space_field: tantivy::schema::Field,
    memory_field: tantivy::schema::Field,
    revision_field: tantivy::schema::Field,
    derived_unit_field: tantivy::schema::Field,
    resolution_field: tantivy::schema::Field,
    path: PathBuf,
    hash: LexicalArtifactHash,
    search_count: AtomicUsize,
}

impl LexicalArtifactHandle {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DerivedError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_dir() {
            return Err(DerivedError::InvalidProjectionValue {
                value: format!(
                    "lexical artifact path is not a directory: {}",
                    path.display()
                ),
            });
        }
        let hash = LexicalArtifactHash::from_bytes(hash_tree(&path)?);
        let index = Index::open_in_dir(&path)?;
        let schema = index.schema();
        let text_field = schema.get_field("text").map_err(DerivedError::from)?;
        let space_field = schema.get_field("space_id").map_err(DerivedError::from)?;
        let memory_field = schema.get_field("memory_id").map_err(DerivedError::from)?;
        let revision_field = schema
            .get_field("revision_id")
            .map_err(DerivedError::from)?;
        let derived_unit_field = schema
            .get_field("derived_unit_id")
            .map_err(DerivedError::from)?;
        let resolution_field = schema.get_field("resolution").map_err(DerivedError::from)?;
        let reader = index.reader()?;
        Ok(Self {
            index,
            reader,
            text_field,
            space_field,
            memory_field,
            revision_field,
            derived_unit_field,
            resolution_field,
            path,
            hash,
            search_count: AtomicUsize::new(0),
        })
    }

    pub fn search(
        &self,
        query_text: &str,
        scope: &[SpaceId],
        limit: usize,
    ) -> Result<Vec<LexicalHit>, DerivedError> {
        self.search_count.fetch_add(1, Ordering::Relaxed);
        if query_text.trim().is_empty() || limit == 0 || scope.is_empty() {
            return Ok(Vec::new());
        }
        let parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let text_query = parser.parse_query(query_text)?;
        let space_query = BooleanQuery::union(
            scope
                .iter()
                .map(|space_id| {
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.space_field, &space_id.to_string()),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>
                })
                .collect(),
        );
        let query = BooleanQuery::intersection(vec![text_query, Box::new(space_query)]);
        let searcher = self.reader.searcher();
        searcher
            .search(&query, &TopDocs::with_limit(limit))?
            .into_iter()
            .map(|(score, address)| {
                let document = searcher.doc(address)?;
                Ok(LexicalHit {
                    score,
                    space_id: stored_id(&document, self.space_field)?.parse()?,
                    memory_id: stored_id(&document, self.memory_field)?.parse()?,
                    revision_id: stored_id(&document, self.revision_field)?.parse()?,
                    derived_unit_id: stored_id(&document, self.derived_unit_field)?.to_owned(),
                    resolution: stored_id(&document, self.resolution_field)?.to_owned(),
                })
            })
            .collect()
    }

    #[must_use]
    pub const fn hash(&self) -> LexicalArtifactHash {
        self.hash
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn search_count(&self) -> usize {
        self.search_count.load(Ordering::Relaxed)
    }
}

fn schema_v1() -> (
    Schema,
    tantivy::schema::Field,
    tantivy::schema::Field,
    tantivy::schema::Field,
    tantivy::schema::Field,
) {
    let mut builder = Schema::builder();
    let text_field = builder.add_text_field("text", TEXT);
    let space_field = builder.add_text_field("space_id", STRING | STORED);
    let memory_field = builder.add_text_field("memory_id", STRING | STORED);
    let revision_field = builder.add_text_field("revision_id", STRING | STORED);
    builder.add_text_field("derived_unit_id", STRING | STORED);
    builder.add_text_field("resolution", STRING | STORED);
    (
        builder.build(),
        text_field,
        space_field,
        memory_field,
        revision_field,
    )
}

fn hash_tree(path: &Path) -> Result<[u8; 32], DerivedError> {
    let mut files = Vec::new();
    collect_files(path, path, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    hasher.update(b"memoria-lexical-artifact-v1\0");
    for (relative, bytes) in files {
        let file_hash: [u8; 32] = Sha256::digest(&bytes).into();
        put_bytes(&mut hasher, relative.as_bytes());
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(file_hash);
    }
    Ok(hasher.finalize().into())
}

fn collect_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), DerivedError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| DerivedError::InvalidProjectionValue {
                    value: "lexical artifact file escaped its root".to_owned(),
                })?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, fs::read(path)?));
        }
    }
    Ok(())
}

fn sync_tree(path: &Path) -> Result<(), DerivedError> {
    let mut directories = BTreeSet::new();
    directories.insert(path.to_path_buf());
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.path().is_file() {
            sync_file(&entry.path())?;
        } else if entry.path().is_dir() {
            sync_tree(&entry.path())?;
            directories.insert(entry.path());
        }
    }
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn sync_file(path: &Path) -> Result<(), DerivedError> {
    match File::open(path).and_then(|file| file.sync_all()) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::InvalidInput | ErrorKind::PermissionDenied | ErrorKind::Unsupported
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn sync_directory(path: &Path) -> Result<(), DerivedError> {
    match File::open(path).and_then(|file| file.sync_all()) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::InvalidInput | ErrorKind::PermissionDenied | ErrorKind::Unsupported
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn put_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}
