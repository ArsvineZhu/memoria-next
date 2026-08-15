use std::ops::Range;

use markdown::{
    MdxSignal, ParseOptions,
    mdast::{AttributeContent, AttributeValue, Node},
    message::Place,
    unist::Position,
};

use crate::source::{ParsedSource, SemanticAttribute, make_attribute, make_element_with_text};
use crate::syntax::MdxError;

const CORE_ELEMENT_NAMES: &[&str] = &[
    "MemoryMeta",
    "Section",
    "Entity",
    "Tag",
    "State",
    "Event",
    "Relation",
    "MemoryRef",
    "Source",
    "Quote",
    "Extension",
];
const MAX_NESTING_DEPTH: usize = 128;

/// Parse restricted-MDX input into the mature markdown-rs syntax tree.
///
/// markdown-rs recognizes expressions using its brace-aware collector. ESM
/// requires a host callback, so this adapter only acknowledges the syntax
/// boundary; it never evaluates or interprets JavaScript.
pub fn parse_mdast(source: &str) -> Result<Node, MdxError> {
    let mut options = ParseOptions::mdx();
    options.mdx_esm_parse = Some(Box::new(|_value| MdxSignal::Ok));
    match markdown::to_mdast(source, &options) {
        Ok(tree) => Ok(tree),
        Err(_error) if source.contains("<!--") => {
            let masked = mask_html_comments(source);
            markdown::to_mdast(&masked, &options).map_err(|error| MdxError::MalformedElement {
                message: error.to_string(),
                span: message_span(&error.place),
            })
        }
        Err(error) => Err(MdxError::MalformedElement {
            message: error.to_string(),
            span: message_span(&error.place),
        }),
    }
}

pub(crate) fn parse_source(source: &str) -> Result<ParsedSource, MdxError> {
    let tree = parse_mdast(source)?;
    let mut elements = Vec::new();
    collect_semantic_elements(source, &tree, 0, &mut elements)?;
    elements.sort_by_key(|element| element.span().start);
    Ok(ParsedSource::new(source.to_owned(), elements))
}

/// Convert markdown-rs's UTF-8 byte offsets into Memoria source spans.
#[must_use]
pub fn mdast_source_span(position: &Position) -> Range<usize> {
    position.start.offset..position.end.offset
}

fn collect_semantic_elements(
    source: &str,
    node: &Node,
    depth: usize,
    elements: &mut Vec<crate::source::SemanticElement>,
) -> Result<(), MdxError> {
    match node {
        Node::MdxjsEsm(element) => {
            return Err(MdxError::ForbiddenDirective {
                span: element
                    .position
                    .as_ref()
                    .map(mdast_source_span)
                    .unwrap_or_default(),
            });
        }
        Node::MdxFlowExpression(element) => {
            return Err(MdxError::ExecutableSyntax {
                span: element
                    .position
                    .as_ref()
                    .map(mdast_source_span)
                    .unwrap_or_default(),
            });
        }
        Node::MdxTextExpression(element) => {
            return Err(MdxError::ExecutableSyntax {
                span: element
                    .position
                    .as_ref()
                    .map(mdast_source_span)
                    .unwrap_or_default(),
            });
        }
        Node::Html(element) => {
            return Err(MdxError::RawHtml {
                name: html_name(&element.value),
                span: element
                    .position
                    .as_ref()
                    .map(mdast_source_span)
                    .unwrap_or_default(),
            });
        }
        Node::MdxJsxFlowElement(element) => {
            let semantic = semantic_element_from_jsx(
                source,
                element.position.as_ref(),
                element.name.as_deref(),
                &element.attributes,
                node,
                depth,
            )?;
            elements.push(semantic);
            for child in &element.children {
                collect_semantic_elements(source, child, depth + 1, elements)?;
            }
            return Ok(());
        }
        Node::MdxJsxTextElement(element) => {
            let semantic = semantic_element_from_jsx(
                source,
                element.position.as_ref(),
                element.name.as_deref(),
                &element.attributes,
                node,
                depth,
            )?;
            elements.push(semantic);
            for child in &element.children {
                collect_semantic_elements(source, child, depth + 1, elements)?;
            }
            return Ok(());
        }
        _ => {}
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_semantic_elements(source, child, depth, elements)?;
        }
    }
    Ok(())
}

fn semantic_element_from_jsx(
    source: &str,
    position: Option<&Position>,
    name: Option<&str>,
    attributes: &[AttributeContent],
    node: &Node,
    depth: usize,
) -> Result<crate::source::SemanticElement, MdxError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(MdxError::ResourceLimit {
            resource: "semantic nesting depth",
        });
    }
    let span = position
        .map(mdast_source_span)
        .ok_or_else(|| MdxError::MalformedElement {
            message: "semantic JSX element has no source position".to_owned(),
            span: 0..0,
        })?;
    let source_fragment = source
        .get(span.clone())
        .ok_or_else(|| MdxError::MalformedElement {
            message: "semantic JSX element position is outside the source".to_owned(),
            span: span.clone(),
        })?;
    let actual_name = name.ok_or_else(|| MdxError::MalformedElement {
        message: "JSX fragments are not part of the restricted profile".to_owned(),
        span: span.clone(),
    })?;
    let semantic_name = semantic_name(actual_name, span.clone())?;
    let converted_attributes = convert_attributes(source, &span, attributes)?;
    let self_closing = source_fragment.trim_end().ends_with("/>");
    let text = if self_closing {
        String::new()
    } else {
        node.to_string()
    };

    Ok(make_element_with_text(
        semantic_name,
        span,
        converted_attributes,
        self_closing,
        text,
    ))
}

fn semantic_name(name: &str, span: Range<usize>) -> Result<String, MdxError> {
    if CORE_ELEMENT_NAMES.contains(&name) {
        return Ok(name.to_owned());
    }
    if name.contains(':') {
        return Ok("Extension".to_owned());
    }
    if name.chars().next().is_some_and(char::is_uppercase) {
        return Err(MdxError::UnsupportedRuntimeComponent {
            name: name.to_owned(),
            span,
        });
    }
    Err(MdxError::RawHtml {
        name: name.to_owned(),
        span,
    })
}

fn convert_attributes(
    source: &str,
    element_span: &Range<usize>,
    attributes: &[AttributeContent],
) -> Result<Vec<SemanticAttribute>, MdxError> {
    let opening_end = opening_tag_end(source, element_span)?;
    let mut cursor = element_span.start + 1;
    let mut result = Vec::with_capacity(attributes.len());
    for attribute in attributes {
        let AttributeContent::Property(attribute) = attribute else {
            return Err(MdxError::ExecutableSyntax {
                span: element_span.clone(),
            });
        };
        let Some(value) = &attribute.value else {
            return Err(MdxError::MalformedElement {
                message: "semantic attributes must use name=quoted-value".to_owned(),
                span: element_span.clone(),
            });
        };
        let AttributeValue::Literal(value) = value else {
            return Err(MdxError::ExecutableSyntax {
                span: element_span.clone(),
            });
        };
        let value_span =
            find_literal_value_span(source, cursor, opening_end, &attribute.name, element_span)?;
        cursor = value_span.end + 1;
        result.push(make_attribute(
            attribute.name.clone(),
            value.clone(),
            value_span,
        ));
    }
    Ok(result)
}

fn opening_tag_end(source: &str, element_span: &Range<usize>) -> Result<usize, MdxError> {
    let bytes = source.as_bytes();
    let mut cursor = element_span.start + 1;
    let mut quote = None;
    while cursor < element_span.end {
        let byte = bytes[cursor];
        match quote {
            Some(delimiter) if byte == delimiter => quote = None,
            Some(_) => {}
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            None if byte == b'>' => return Ok(cursor + 1),
            None => {}
        }
        cursor += 1;
    }
    Err(MdxError::MalformedElement {
        message: "semantic element opening tag is not closed".to_owned(),
        span: element_span.clone(),
    })
}

fn find_literal_value_span(
    source: &str,
    cursor: usize,
    opening_end: usize,
    attribute_name: &str,
    element_span: &Range<usize>,
) -> Result<Range<usize>, MdxError> {
    let name_start =
        find_attribute_start(source, cursor, opening_end, attribute_name).ok_or_else(|| {
            MdxError::MalformedElement {
                message: format!("attribute `{attribute_name}` is missing from its source span"),
                span: element_span.clone(),
            }
        })?;
    let mut position = name_start + attribute_name.len();
    while position < opening_end && source.as_bytes()[position].is_ascii_whitespace() {
        position += 1;
    }
    if position >= opening_end || source.as_bytes()[position] != b'=' {
        return Err(MdxError::MalformedElement {
            message: "semantic attributes must use name=quoted-value".to_owned(),
            span: element_span.clone(),
        });
    }
    position += 1;
    while position < opening_end && source.as_bytes()[position].is_ascii_whitespace() {
        position += 1;
    }
    let delimiter = *source
        .as_bytes()
        .get(position)
        .ok_or_else(|| MdxError::MalformedElement {
            message: "attribute value is missing".to_owned(),
            span: element_span.clone(),
        })?;
    if delimiter != b'\'' && delimiter != b'"' {
        return Err(MdxError::ExecutableSyntax {
            span: position..position + 1,
        });
    }
    let value_start = position + 1;
    let value_end = source[value_start..opening_end]
        .find(delimiter as char)
        .map(|offset| value_start + offset)
        .ok_or_else(|| MdxError::MalformedElement {
            message: "attribute value is not closed".to_owned(),
            span: element_span.clone(),
        })?;
    Ok(value_start..value_end)
}

fn find_attribute_start(
    source: &str,
    mut cursor: usize,
    opening_end: usize,
    attribute_name: &str,
) -> Option<usize> {
    while cursor < opening_end {
        let relative = source[cursor..opening_end].find(attribute_name)?;
        let name_start = cursor + relative;
        let before_is_space =
            name_start > 0 && source.as_bytes()[name_start - 1].is_ascii_whitespace();
        let after = name_start + attribute_name.len();
        let after_is_boundary = after < opening_end
            && (source.as_bytes()[after].is_ascii_whitespace() || source.as_bytes()[after] == b'=');
        if before_is_space && after_is_boundary {
            return Some(name_start);
        }
        cursor = name_start + 1;
    }
    None
}

fn html_name(value: &str) -> String {
    value
        .strip_prefix('<')
        .and_then(|value| {
            value
                .split(|character: char| character.is_ascii_whitespace() || character == '>')
                .next()
        })
        .unwrap_or(value)
        .trim_start_matches('/')
        .to_owned()
}

fn message_span(place: &Option<Box<Place>>) -> Range<usize> {
    match place.as_deref() {
        Some(Place::Position(position)) => mdast_source_span(position),
        Some(Place::Point(point)) => point.offset..point.offset,
        None => 0..0,
    }
}

fn mask_html_comments(source: &str) -> String {
    let mut masked = source.as_bytes().to_vec();
    let mut cursor = 0;
    while let Some(relative_start) = source[cursor..].find("<!--") {
        let start = cursor + relative_start;
        let Some(relative_end) = source[start + 4..].find("-->") else {
            break;
        };
        let end = start + 4 + relative_end + 3;
        for byte in &mut masked[start..end] {
            if *byte != b'\n' && *byte != b'\r' {
                *byte = b' ';
            }
        }
        cursor = end;
    }
    String::from_utf8(masked).expect("masking ASCII comment bytes preserves UTF-8")
}
