use sha2::{Digest, Sha256};

use crate::ir::{
    IrNode, IrTemporalFields, MEMORY_IR_VERSION, MemoryIr, SemanticHash, SourceMapping,
};
use crate::profile::ValidatedDocument;
use crate::syntax::MdxError;
use crate::time::{TemporalPrecision, TemporalValue};

const SEMANTIC_HASH_DOMAIN: &[u8] = b"memoria-semantic-v1\0";

pub fn compile_ir(source: &str) -> Result<MemoryIr, MdxError> {
    #[cfg(any(test, feature = "test-support"))]
    crate::test_support::record_ir_compile_call();

    let document = crate::parse_and_validate(source)?;
    compile_validated(&document)
}

pub fn compile_validated(document: &ValidatedDocument) -> Result<MemoryIr, MdxError> {
    let mut nodes = Vec::new();
    let mut source_mappings = Vec::new();
    for node in document.nodes() {
        let id = node.id().cloned();
        let source_mapping = SourceMapping::new(node.kind(), id.clone(), node.span());
        let attributes = node
            .attributes()
            .iter()
            .map(|(name, value)| (name.clone(), canonical_attribute(name, value)))
            .collect::<Vec<_>>();
        nodes.push(IrNode::new(
            node.kind(),
            id,
            attributes,
            canonicalize_visible_text(node.text()),
            IrTemporalFields {
                occurred_at: node.occurred_at().cloned(),
                observed_at: node.observed_at().cloned(),
                valid_from: node.valid_from().cloned(),
                valid_to: node.valid_to().cloned(),
            },
            source_mapping.clone(),
        ));
        source_mappings.push(source_mapping);
    }

    let text_hierarchy = canonical_text_hierarchy(document.source(), &source_mappings);
    let canonical_bytes = encode_canonical(&text_hierarchy, &nodes);
    let semantic_hash = hash_canonical(&canonical_bytes);
    Ok(MemoryIr::new(
        canonical_bytes,
        semantic_hash,
        text_hierarchy,
        nodes,
        source_mappings,
    ))
}

fn encode_canonical(text_hierarchy: &str, nodes: &[IrNode]) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(b"memoria-ir-v1\0");
    output.extend_from_slice(&MEMORY_IR_VERSION.to_be_bytes());
    put_string(&mut output, text_hierarchy);
    put_u32(&mut output, nodes.len());
    for node in nodes {
        put_string(&mut output, node.kind().as_str());
        put_optional_string(&mut output, node.id().map(ToString::to_string).as_deref());
        put_u32(&mut output, node.attributes().len());
        for (name, value) in node.attributes() {
            put_string(&mut output, name);
            put_string(&mut output, value);
        }
        put_string(&mut output, node.text());
        put_temporal(&mut output, node.occurred_at());
        put_temporal(&mut output, node.observed_at());
        put_temporal(&mut output, node.valid_from());
        put_temporal(&mut output, node.valid_to());
    }
    output
}

fn hash_canonical(canonical_bytes: &[u8]) -> SemanticHash {
    let mut hasher = Sha256::new();
    hasher.update(SEMANTIC_HASH_DOMAIN);
    hasher.update(canonical_bytes);
    SemanticHash::from_bytes(hasher.finalize().into())
}

fn put_string(output: &mut Vec<u8>, value: &str) {
    put_u32(output, value.len());
    output.extend_from_slice(value.as_bytes());
}

fn put_optional_string(output: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            output.push(1);
            put_string(output, value);
        }
        None => output.push(0),
    }
}

fn put_u32(output: &mut Vec<u8>, value: usize) {
    output.extend_from_slice(&(value as u32).to_be_bytes());
}

fn put_temporal(output: &mut Vec<u8>, value: Option<&TemporalValue>) {
    match value {
        Some(value) => {
            output.push(1);
            output.push(temporal_precision(value.precision()));
            put_string(output, value.raw());
        }
        None => output.push(0),
    }
}

fn temporal_precision(precision: TemporalPrecision) -> u8 {
    match precision {
        TemporalPrecision::Year => 1,
        TemporalPrecision::Month => 2,
        TemporalPrecision::Day => 3,
        TemporalPrecision::Timestamp => 4,
    }
}

fn canonical_attribute(name: &str, value: &str) -> String {
    match name {
        "occurredAt" | "observedAt" | "validFrom" | "validTo" => value.to_owned(),
        _ => value.to_owned(),
    }
}

fn canonical_text_hierarchy(source: &str, mappings: &[SourceMapping]) -> String {
    let mut outside = String::new();
    let mut cursor = 0;
    for mapping in mappings {
        let span = mapping.span();
        if span.start >= cursor {
            outside.push_str(&source[cursor..span.start]);
        }
        cursor = cursor.max(span.end);
    }
    if cursor < source.len() {
        outside.push_str(&source[cursor..]);
    }
    normalize_markdown_text(&strip_comments(&outside))
}

fn canonicalize_visible_text(value: &str) -> String {
    normalize_markdown_text(&strip_markup(value))
}

fn normalize_markdown_text(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in normalized.split('\n') {
        let line = line.trim_end_matches([' ', '\t']);
        let blank = line.is_empty();
        if blank && previous_blank {
            continue;
        }
        previous_blank = blank;
        lines.push(line);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn strip_comments(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        if value[cursor..].starts_with("<!--") {
            if let Some(end) = value[cursor + 4..].find("-->") {
                cursor += 4 + end + 3;
            } else {
                break;
            }
        } else if let Some(character) = value[cursor..].chars().next() {
            output.push(character);
            cursor += character.len_utf8();
        } else {
            break;
        }
    }
    output
}

fn strip_markup(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;
    while cursor < value.len() {
        if value[cursor..].starts_with("<!--")
            && let Some(end) = value[cursor + 4..].find("-->")
        {
            cursor += 4 + end + 3;
            continue;
        }
        if value.as_bytes()[cursor] == b'<'
            && let Some(end) = markup_end(value, cursor)
        {
            cursor = end;
            continue;
        }
        if let Some(character) = value[cursor..].chars().next() {
            output.push(character);
            cursor += character.len_utf8();
        } else {
            break;
        }
    }
    output
}

fn markup_end(value: &str, start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    let mut quote = None;
    while cursor < value.len() {
        let byte = value.as_bytes()[cursor];
        match quote {
            Some(delimiter) if byte == delimiter => quote = None,
            Some(_) => {}
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            None if byte == b'>' => return Some(cursor + 1),
            None => {}
        }
        cursor += 1;
    }
    None
}
