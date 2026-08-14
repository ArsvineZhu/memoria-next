use std::ops::Range;

use crate::source::{ParsedSource, SemanticAttribute, make_attribute, make_element};
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

#[derive(Clone, Debug)]
struct OpenElement {
    name: String,
    start: usize,
    attributes: Vec<SemanticAttribute>,
}

pub(crate) fn lex_source(
    source: &str,
    protected: &[Range<usize>],
) -> Result<ParsedSource, MdxError> {
    reject_forbidden_syntax(source, protected)?;
    let mut elements = Vec::new();
    let mut stack = Vec::<OpenElement>::new();
    let bytes = source.as_bytes();
    let mut cursor = 0;

    while cursor < bytes.len() {
        if bytes[cursor] != b'<' || in_protected(cursor, protected) {
            cursor += 1;
            continue;
        }
        if source.is_char_boundary(cursor) && source[cursor..].starts_with("<!--") {
            cursor = source[cursor + 4..]
                .find("-->")
                .map(|end| cursor + 4 + end + 3)
                .unwrap_or(bytes.len());
            continue;
        }
        let (tag_end, tag_text) = scan_tag(source, cursor)?;
        if tag_text.starts_with('!') || tag_text.starts_with('?') {
            return Err(MdxError::RawHtml {
                name: tag_text
                    .split_ascii_whitespace()
                    .next()
                    .unwrap_or(tag_text)
                    .to_owned(),
                span: cursor..tag_end,
            });
        }
        if let Some(closing_name) = tag_text.strip_prefix('/') {
            let name = parse_name(closing_name.trim(), cursor..tag_end)?;
            if is_core_name(&name) {
                let Some(open) = stack.pop() else {
                    return Err(MdxError::MismatchedClosingElement {
                        expected: "no open semantic element".to_owned(),
                        found: name,
                        span: cursor..tag_end,
                    });
                };
                if open.name != name {
                    return Err(MdxError::MismatchedClosingElement {
                        expected: open.name,
                        found: name,
                        span: cursor..tag_end,
                    });
                }
                elements.push(make_element(
                    open.name,
                    open.start..tag_end,
                    open.attributes,
                    false,
                ));
            } else {
                return Err(MdxError::RawHtml {
                    name,
                    span: cursor..tag_end,
                });
            }
            cursor = tag_end;
            continue;
        }

        let (name, attributes, self_closing) = parse_open_tag(tag_text, cursor..tag_end)?;
        if !is_uppercase_name(&name) {
            return Err(MdxError::RawHtml {
                name,
                span: cursor..tag_end,
            });
        }
        if !is_core_name(&name) {
            return Err(MdxError::UnsupportedRuntimeComponent {
                name,
                span: cursor..tag_end,
            });
        }
        if self_closing {
            elements.push(make_element(name, cursor..tag_end, attributes, true));
        } else {
            if stack.len() >= MAX_NESTING_DEPTH {
                return Err(MdxError::ResourceLimit {
                    resource: "semantic nesting depth",
                });
            }
            stack.push(OpenElement {
                name,
                start: cursor,
                attributes,
            });
        }
        cursor = tag_end;
    }

    if let Some(open) = stack.pop() {
        return Err(MdxError::UnclosedElement {
            name: open.name,
            span: open.start..bytes.len(),
        });
    }
    elements.sort_by_key(|element| element.span().start);
    Ok(ParsedSource::new(source.to_owned(), elements))
}

fn reject_forbidden_syntax(source: &str, protected: &[Range<usize>]) -> Result<(), MdxError> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if in_protected(cursor, protected) {
            cursor += 1;
            continue;
        }
        if source.is_char_boundary(cursor) && source[cursor..].starts_with("<!--") {
            cursor = source[cursor + 4..]
                .find("-->")
                .map(|end| cursor + 4 + end + 3)
                .unwrap_or(bytes.len());
            continue;
        }
        if bytes[cursor] == b'{' || bytes[cursor] == b'}' {
            return Err(MdxError::ExecutableSyntax {
                span: cursor..cursor + 1,
            });
        }
        if source.is_char_boundary(cursor)
            && (source[cursor..].starts_with("import ")
                || source[cursor..].starts_with("export ")
                || source[cursor..].starts_with("import{")
                || source[cursor..].starts_with("export{"))
        {
            return Err(MdxError::ForbiddenDirective {
                span: cursor..(cursor + 6).min(bytes.len()),
            });
        }
        cursor += 1;
    }
    Ok(())
}

fn scan_tag(source: &str, start: usize) -> Result<(usize, &str), MdxError> {
    let bytes = source.as_bytes();
    let mut cursor = start + 1;
    let mut quote = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        match quote {
            Some(delimiter) if byte == delimiter => quote = None,
            Some(_) => {}
            None if byte == b'\'' || byte == b'"' => quote = Some(byte),
            None if byte == b'>' => return Ok((cursor + 1, &source[start + 1..cursor])),
            None => {}
        }
        cursor += 1;
    }
    Err(malformed("tag is not closed", start..bytes.len()))
}

fn parse_open_tag(
    text: &str,
    span: Range<usize>,
) -> Result<(String, Vec<SemanticAttribute>, bool), MdxError> {
    let mut rest = text.trim();
    let self_closing = rest.ends_with('/');
    if self_closing {
        rest = rest[..rest.len() - 1].trim_end();
    }
    let (name, consumed) = parse_name_with_length(rest, span.clone())?;
    let mut cursor = consumed;
    let mut attributes = Vec::new();
    while cursor < rest.len() {
        while cursor < rest.len() && rest.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == rest.len() {
            break;
        }
        let (attribute_name, name_length) = parse_name_with_length(&rest[cursor..], span.clone())?;
        cursor += name_length;
        while cursor < rest.len() && rest.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= rest.len() || rest.as_bytes()[cursor] != b'=' {
            return Err(malformed(
                "semantic attributes must use name=quoted-value",
                span,
            ));
        }
        cursor += 1;
        while cursor < rest.len() && rest.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let delimiter = *rest
            .as_bytes()
            .get(cursor)
            .ok_or_else(|| malformed("attribute value is missing", span.clone()))?;
        if delimiter != b'\'' && delimiter != b'"' {
            return Err(MdxError::ExecutableSyntax {
                span: span.start + 1 + cursor..span.start + 1 + cursor + 1,
            });
        }
        let value_start = cursor + 1;
        cursor += 1;
        let value_end = rest[cursor..]
            .find(delimiter as char)
            .map(|end| cursor + end)
            .ok_or_else(|| malformed("attribute value is not closed", span.clone()))?;
        let value = &rest[value_start..value_end];
        if value.contains('{') || value.contains('}') {
            return Err(MdxError::ExecutableSyntax {
                span: span.start + value_start..span.start + value_end,
            });
        }
        attributes.push(make_attribute(
            attribute_name,
            value.to_owned(),
            span.start + 1 + value_start..span.start + 1 + value_end,
        ));
        cursor = value_end + 1;
    }
    Ok((name, attributes, self_closing))
}

fn parse_name(text: &str, span: Range<usize>) -> Result<String, MdxError> {
    Ok(parse_name_with_length(text, span)?.0)
}

fn parse_name_with_length(text: &str, span: Range<usize>) -> Result<(String, usize), MdxError> {
    let bytes = text.as_bytes();
    if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return Err(malformed("tag or attribute name is missing", span));
    }
    let mut end = 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_' | b'-' | b':' | b'.'))
    {
        end += 1;
    }
    Ok((text[..end].to_owned(), end))
}

fn is_uppercase_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

fn is_core_name(name: &str) -> bool {
    CORE_ELEMENT_NAMES.contains(&name)
}

fn in_protected(position: usize, protected: &[Range<usize>]) -> bool {
    protected
        .iter()
        .any(|range| range.start <= position && position < range.end)
}

fn malformed(message: impl Into<String>, span: Range<usize>) -> MdxError {
    MdxError::MalformedElement {
        message: message.into(),
        span,
    }
}
