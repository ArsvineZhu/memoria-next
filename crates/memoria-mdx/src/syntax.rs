use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MdxError {
    #[error("executable MDX syntax is not allowed at byte span {span:?}")]
    ExecutableSyntax { span: Range<usize> },

    #[error("runtime component `{name}` is not allowed at byte span {span:?}")]
    UnsupportedRuntimeComponent { name: String, span: Range<usize> },

    #[error("malformed semantic element at byte span {span:?}: {message}")]
    MalformedElement { message: String, span: Range<usize> },

    #[error("semantic element `{name}` is not closed")]
    UnclosedElement { name: String, span: Range<usize> },

    #[error("semantic element closing tag does not match `{expected}`")]
    MismatchedClosingElement {
        expected: String,
        found: String,
        span: Range<usize>,
    },

    #[error("forbidden MDX directive at byte span {span:?}")]
    ForbiddenDirective { span: Range<usize> },
}
