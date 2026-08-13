mod semantic_lexer;
mod source;
mod syntax;

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

pub use source::{ParsedSource, SemanticAttribute, SemanticElement};
pub use syntax::MdxError;

pub fn parse_source(source: &str) -> Result<ParsedSource, MdxError> {
    let protected = markdown_protected_ranges(source);
    semantic_lexer::lex_source(source, &protected)
}

fn markdown_protected_ranges(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut code_block_start = None;
    for (event, range) in Parser::new(source).into_offset_iter() {
        match event {
            Event::Code(_) => ranges.push(range),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
            | Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                code_block_start = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = code_block_start.take() {
                    ranges.push(start..range.end);
                }
            }
            _ => {}
        }
    }
    ranges
}
