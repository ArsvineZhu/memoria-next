use std::ops::Range;

use markdown::{MdxSignal, ParseOptions, mdast::Node, message::Place, unist::Position};

use crate::syntax::MdxError;

/// Parse restricted-MDX input into the mature markdown-rs syntax tree.
///
/// markdown-rs recognizes expressions using its brace-aware collector. ESM
/// requires a host callback, so this adapter only acknowledges the syntax
/// boundary; it never evaluates or interprets JavaScript.
pub fn parse_mdast(source: &str) -> Result<Node, MdxError> {
    let mut options = ParseOptions::mdx();
    options.mdx_esm_parse = Some(Box::new(|_value| MdxSignal::Ok));
    markdown::to_mdast(source, &options).map_err(|error| MdxError::MalformedElement {
        message: error.to_string(),
        span: message_span(&error.place),
    })
}

/// Convert markdown-rs's UTF-8 byte offsets into Memoria source spans.
#[must_use]
pub fn mdast_source_span(position: &Position) -> Range<usize> {
    position.start.offset..position.end.offset
}

fn message_span(place: &Option<Box<Place>>) -> Range<usize> {
    match place.as_deref() {
        Some(Place::Position(position)) => mdast_source_span(position),
        Some(Place::Point(point)) => point.offset..point.offset,
        None => 0..0,
    }
}
