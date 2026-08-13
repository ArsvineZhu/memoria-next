use std::collections::{BTreeMap, HashMap, HashSet};

use crate::parse_source;
use crate::profile::{SemanticKind, SemanticNodeId, ValidatedDocument, ValidatedNode};
use crate::source::{ParsedSource, SemanticElement};
use crate::syntax::MdxError;
use crate::time::TemporalValue;

const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_SEMANTIC_ELEMENTS: usize = 4096;
const MAX_ATTRIBUTES_PER_ELEMENT: usize = 64;
const MAX_ELEMENT_TEXT_BYTES: usize = 1024 * 1024;
const MAX_NODE_ID_BYTES: usize = 256;
const MAX_ENTITY_REF_BYTES: usize = 512;
const MAX_SOURCE_LOCATOR_BYTES: usize = 2048;

pub fn parse_and_validate(source: &str) -> Result<ValidatedDocument, MdxError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(MdxError::ResourceLimit {
            resource: "source bytes",
        });
    }
    let parsed = parse_source(source)?;
    validate_parsed(parsed)
}

pub fn validate_parsed(parsed: ParsedSource) -> Result<ValidatedDocument, MdxError> {
    let mut nodes = Vec::new();
    let mut ids = HashMap::<SemanticNodeId, std::ops::Range<usize>>::new();
    let source = parsed.source().to_owned();
    for element in parsed.semantic_elements() {
        if nodes.len() >= MAX_SEMANTIC_ELEMENTS {
            return Err(MdxError::ResourceLimit {
                resource: "semantic elements",
            });
        }
        if element.attributes().len() > MAX_ATTRIBUTES_PER_ELEMENT {
            return Err(MdxError::ResourceLimit {
                resource: "attributes per semantic element",
            });
        }
        let kind = SemanticKind::parse(element);
        let attributes = collect_attributes(element)?;
        let id = optional_node_id(element, &attributes)?;
        if let Some(id) = &id
            && let Some(first_span) = ids.insert(id.clone(), element.span())
        {
            return Err(MdxError::DuplicateSemanticNodeId {
                id: id.to_string(),
                first_span,
                duplicate_span: element.span(),
            });
        }
        validate_required_attributes(element, kind, &attributes)?;
        validate_references(element, kind, &attributes)?;
        let occurred_at = parse_temporal_attribute(element, &attributes, "occurredAt")?;
        let observed_at = parse_temporal_attribute(element, &attributes, "observedAt")?;
        let valid_from = parse_temporal_attribute(element, &attributes, "validFrom")?;
        let valid_to = parse_temporal_attribute(element, &attributes, "validTo")?;
        if let (Some(start), Some(end)) = (&valid_from, &valid_to)
            && temporal_key(start) >= temporal_key(end)
        {
            return Err(MdxError::InvalidTemporalValue {
                value: format!("validTo `{end}` must be after validFrom `{start}`"),
                span: element.span(),
            });
        }
        let text = element_text(&source, element)?;
        if text.len() > MAX_ELEMENT_TEXT_BYTES {
            return Err(MdxError::ResourceLimit {
                resource: "semantic element text",
            });
        }
        nodes.push(ValidatedNode::from_parts(
            kind,
            id,
            attributes,
            text,
            element.span(),
            occurred_at,
            observed_at,
            valid_from,
            valid_to,
        ));
    }
    validate_cross_references(&nodes)?;
    Ok(ValidatedDocument::new(parsed, nodes))
}

fn collect_attributes(element: &SemanticElement) -> Result<BTreeMap<String, String>, MdxError> {
    let mut attributes = BTreeMap::new();
    for attribute in element.attributes() {
        if attributes
            .insert(attribute.name().to_owned(), attribute.value().to_owned())
            .is_some()
        {
            return Err(MdxError::MalformedElement {
                message: format!("duplicate attribute `{}`", attribute.name()),
                span: element.span(),
            });
        }
    }
    if let Some(kind) = attributes.get("kind")
        && !known_kind(kind)
    {
        return Err(MdxError::UnknownKind {
            value: kind.clone(),
            span: element.span(),
        });
    }
    Ok(attributes)
}

fn optional_node_id(
    element: &SemanticElement,
    attributes: &BTreeMap<String, String>,
) -> Result<Option<SemanticNodeId>, MdxError> {
    let required = matches!(element.name(), "State" | "Event" | "Relation" | "Source");
    let Some(value) = attributes.get("id") else {
        if required {
            return Err(MdxError::MissingAttribute {
                element: element.name().to_owned(),
                attribute: "id".to_owned(),
                span: element.span(),
            });
        }
        return Ok(None);
    };
    if value.len() > MAX_NODE_ID_BYTES {
        return Err(MdxError::InvalidNodeId {
            value: value.clone(),
            span: element.span(),
        });
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| MdxError::InvalidNodeId {
            value: value.clone(),
            span: element.span(),
        })
}

fn validate_required_attributes(
    element: &SemanticElement,
    kind: SemanticKind,
    attributes: &BTreeMap<String, String>,
) -> Result<(), MdxError> {
    let required = match kind {
        SemanticKind::Entity => &["ref"][..],
        SemanticKind::Tag => &["value"][..],
        SemanticKind::State => &[][..],
        SemanticKind::Event => &["occurredAt"][..],
        SemanticKind::Relation => &["from", "to"][..],
        SemanticKind::Source => &["ref"][..],
        SemanticKind::Quote => &[][..],
        SemanticKind::Extension => &["namespace", "type", "version"][..],
        SemanticKind::MemoryMeta | SemanticKind::Section | SemanticKind::MemoryRef => &[][..],
    };
    for attribute in required {
        if !attributes.contains_key(*attribute) {
            return Err(MdxError::MissingAttribute {
                element: element.name().to_owned(),
                attribute: (*attribute).to_owned(),
                span: element.span(),
            });
        }
    }
    if kind == SemanticKind::Extension {
        let namespace = attributes.get("namespace").expect("required above");
        let extension_type = attributes.get("type").expect("required above");
        let version = attributes.get("version").expect("required above");
        if !valid_extension_namespace(namespace) {
            return Err(MdxError::InvalidExtension {
                message: "namespace must be a dot-qualified lowercase token".to_owned(),
                span: element.span(),
            });
        }
        if !valid_extension_token(extension_type) {
            return Err(MdxError::InvalidExtension {
                message: "type must be a non-empty token".to_owned(),
                span: element.span(),
            });
        }
        if version.parse::<u32>().is_err() {
            return Err(MdxError::InvalidExtension {
                message: "version must be an unsigned integer".to_owned(),
                span: element.span(),
            });
        }
    }
    Ok(())
}

fn validate_references(
    element: &SemanticElement,
    kind: SemanticKind,
    attributes: &BTreeMap<String, String>,
) -> Result<(), MdxError> {
    if matches!(kind, SemanticKind::Entity) {
        let value = attributes.get("ref").expect("required above");
        validate_entity_ref(value).map_err(|_| MdxError::InvalidEntityRef {
            value: value.clone(),
            span: element.span(),
        })?;
    }
    for attribute in ["about", "speaker"] {
        if let Some(value) = attributes.get(attribute) {
            validate_entity_ref(value).map_err(|_| MdxError::InvalidEntityRef {
                value: value.clone(),
                span: element.span(),
            })?;
        }
    }
    if matches!(kind, SemanticKind::Source) {
        let value = attributes.get("ref").expect("required above");
        if !valid_source_locator(value) {
            return Err(MdxError::InvalidReference {
                value: value.clone(),
                span: element.span(),
            });
        }
    }
    if kind == SemanticKind::Relation {
        for attribute in ["from", "to"] {
            let value = attributes.get(attribute).expect("required above");
            if !value.starts_with('#') || value[1..].parse::<SemanticNodeId>().is_err() {
                return Err(MdxError::InvalidReference {
                    value: value.clone(),
                    span: element.span(),
                });
            }
        }
    }
    if kind == SemanticKind::Quote
        && let Some(value) = attributes.get("source")
        && local_reference_id(value).is_none()
    {
        return Err(MdxError::InvalidReference {
            value: value.clone(),
            span: element.span(),
        });
    }
    Ok(())
}

fn validate_cross_references(nodes: &[ValidatedNode]) -> Result<(), MdxError> {
    let ids = nodes
        .iter()
        .filter_map(|node| node.id())
        .cloned()
        .collect::<HashSet<_>>();
    for node in nodes {
        if node.kind() == SemanticKind::Relation {
            for attribute in ["from", "to"] {
                let value = node.attributes().get(attribute).expect("required above");
                let id = &value[1..];
                if !ids.iter().any(|candidate| candidate.as_str() == id) {
                    return Err(MdxError::InvalidReference {
                        value: value.clone(),
                        span: node.span(),
                    });
                }
            }
        }
        if node.kind() == SemanticKind::Quote
            && let Some(value) = node.attributes().get("source")
            && let Some(id) = local_reference_id(value)
            && !ids.iter().any(|candidate| candidate.as_str() == id)
        {
            return Err(MdxError::InvalidReference {
                value: value.clone(),
                span: node.span(),
            });
        }
    }
    Ok(())
}

fn parse_temporal_attribute(
    element: &SemanticElement,
    attributes: &BTreeMap<String, String>,
    name: &str,
) -> Result<Option<TemporalValue>, MdxError> {
    attributes
        .get(name)
        .map(|value| {
            value.parse().map_err(|_| MdxError::InvalidTemporalValue {
                value: value.clone(),
                span: element.span(),
            })
        })
        .transpose()
}

fn element_text(source: &str, element: &SemanticElement) -> Result<String, MdxError> {
    let span = element.span();
    let Some(open_end_relative) = source[span.clone()].find('>') else {
        return Err(MdxError::MalformedElement {
            message: "element opening tag is not closed".to_owned(),
            span,
        });
    };
    let open_end = span.start + open_end_relative + 1;
    if element.self_closing() {
        return Ok(String::new());
    }
    let Some(close_start_relative) = source[open_end..span.end].rfind("</") else {
        return Err(MdxError::UnclosedElement {
            name: element.name().to_owned(),
            span,
        });
    };
    Ok(source[open_end..open_end + close_start_relative].to_owned())
}

fn validate_entity_ref(value: &str) -> Result<(), ()> {
    if value.is_empty() || value.len() > MAX_ENTITY_REF_BYTES {
        return Err(());
    }
    let Some((namespace, opaque)) = value.split_once(':') else {
        return Err(());
    };
    if namespace.is_empty()
        || opaque.is_empty()
        || !namespace
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
        || !namespace.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        || opaque.chars().any(char::is_whitespace)
    {
        return Err(());
    }
    Ok(())
}

fn valid_source_locator(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SOURCE_LOCATOR_BYTES
        && !value.chars().any(char::is_whitespace)
}

fn local_reference_id(value: &str) -> Option<&str> {
    let id = value.strip_prefix('#').unwrap_or(value);
    if id.parse::<SemanticNodeId>().is_ok() {
        Some(id)
    } else {
        None
    }
}

fn valid_extension_namespace(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.split('.').all(valid_extension_token)
}

fn valid_extension_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}

fn known_kind(value: &str) -> bool {
    matches!(
        value,
        "association"
            | "causal"
            | "contradiction"
            | "supports"
            | "part_of"
            | "related"
            | "person"
            | "place"
            | "organization"
            | "project"
            | "thing"
            | "decision"
            | "status"
            | "fact"
            | "note"
            | "reference"
    )
}

fn temporal_key(value: &TemporalValue) -> (i32, u8, u8, u8) {
    (
        value.year_value(),
        value.month_value().unwrap_or(1),
        value.day_value().unwrap_or(1),
        value.precision() as u8,
    )
}
