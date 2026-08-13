use memoria_mdx::{IrNode, MemoryIr, SemanticKind};
use memoria_types::{MemoryId, RevisionId, SpaceId};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, TantivyDocument, doc};

use crate::DerivedError;
use crate::dependency::{ProjectionInputHash, ProjectionKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexicalDocument {
    space_id: SpaceId,
    memory_id: MemoryId,
    revision_id: RevisionId,
    ir: MemoryIr,
}

impl LexicalDocument {
    #[must_use]
    pub fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        ir: MemoryIr,
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            ir,
        }
    }

    #[must_use]
    pub const fn space_id(&self) -> SpaceId {
        self.space_id
    }

    #[must_use]
    pub const fn memory_id(&self) -> MemoryId {
        self.memory_id
    }

    #[must_use]
    pub const fn revision_id(&self) -> RevisionId {
        self.revision_id
    }

    #[must_use]
    pub const fn ir(&self) -> &MemoryIr {
        &self.ir
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexicalHit {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub score: f32,
}

pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    text_field: tantivy::schema::Field,
    space_field: tantivy::schema::Field,
    memory_field: tantivy::schema::Field,
    revision_field: tantivy::schema::Field,
    projection_hash: ProjectionInputHash,
}

pub fn build_lexical(document: &LexicalDocument) -> Result<LexicalIndex, DerivedError> {
    let (schema, text_field, space_field, memory_field, revision_field) = build_schema();
    let index = Index::create_in_ram(schema);
    let mut writer = index.writer(50_000_000)?;
    let text = lexical_text(document.ir());
    writer.add_document(doc!(
        text_field => text,
        space_field => document.space_id().to_string(),
        memory_field => document.memory_id().to_string(),
        revision_field => document.revision_id().to_string(),
    ))?;
    writer.commit()?;
    let reader = index.reader()?;
    Ok(LexicalIndex {
        index,
        reader,
        text_field,
        space_field,
        memory_field,
        revision_field,
        projection_hash: lexical_projection_hash(document),
    })
}

impl LexicalIndex {
    pub fn search(&self, query_text: &str, limit: usize) -> Result<Vec<LexicalHit>, DerivedError> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let query = parser.parse_query(query_text)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        top_docs
            .into_iter()
            .map(|(score, address)| {
                let document = searcher.doc(address)?;
                Ok(LexicalHit {
                    score,
                    space_id: stored_id(&document, self.space_field)?.parse()?,
                    memory_id: stored_id(&document, self.memory_field)?.parse()?,
                    revision_id: stored_id(&document, self.revision_field)?.parse()?,
                })
            })
            .collect()
    }

    #[must_use]
    pub fn projection_hash(&self) -> &ProjectionInputHash {
        &self.projection_hash
    }
}

#[must_use]
pub fn lexical_projection_hash(document: &LexicalDocument) -> ProjectionInputHash {
    ProjectionInputHash::new(
        ProjectionKind::Lexical,
        &lexical_projection_bytes(document.ir()),
        "tantivy-v1",
    )
}

fn build_schema() -> (
    Schema,
    tantivy::schema::Field,
    tantivy::schema::Field,
    tantivy::schema::Field,
    tantivy::schema::Field,
) {
    let mut builder = Schema::builder();
    let text_field = builder.add_text_field("text", TEXT | STORED);
    let space_field = builder.add_text_field("space_id", STRING | STORED);
    let memory_field = builder.add_text_field("memory_id", STRING | STORED);
    let revision_field = builder.add_text_field("revision_id", STRING | STORED);
    (
        builder.build(),
        text_field,
        space_field,
        memory_field,
        revision_field,
    )
}

fn stored_id(
    document: &TantivyDocument,
    field: tantivy::schema::Field,
) -> Result<&str, DerivedError> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .ok_or_else(|| DerivedError::InvalidProjectionValue {
            value: "lexical target ID is missing from stored document".to_owned(),
        })
}

fn lexical_text(ir: &MemoryIr) -> String {
    let mut output = ir.text_hierarchy().to_owned();
    for node in ir.nodes() {
        if node.kind() != SemanticKind::Tag {
            append_text(&mut output, node);
        }
    }
    output
}

fn append_text(output: &mut String, node: &IrNode) {
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(node.text());
}

fn lexical_projection_bytes(ir: &MemoryIr) -> Vec<u8> {
    let mut bytes = Vec::new();
    put_string(&mut bytes, ir.text_hierarchy());
    for node in ir.nodes().filter(|node| node.kind() != SemanticKind::Tag) {
        put_string(&mut bytes, node.kind().as_str());
        put_string(
            &mut bytes,
            node.id().map(ToString::to_string).as_deref().unwrap_or(""),
        );
        put_string(&mut bytes, node.text());
    }
    bytes
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}
