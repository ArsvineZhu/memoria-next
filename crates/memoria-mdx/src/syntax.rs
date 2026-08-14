use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MdxError {
    #[error("executable MDX syntax is not allowed at byte span {span:?}")]
    ExecutableSyntax { span: Range<usize> },

    #[error("runtime component `{name}` is not allowed at byte span {span:?}")]
    UnsupportedRuntimeComponent { name: String, span: Range<usize> },

    #[error("raw HTML tag `{name}` is not allowed at byte span {span:?}")]
    RawHtml { name: String, span: Range<usize> },

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

    #[error("semantic node id `{id}` is declared more than once")]
    DuplicateSemanticNodeId {
        id: String,
        first_span: Range<usize>,
        duplicate_span: Range<usize>,
    },

    #[error("invalid semantic node id `{value}`")]
    InvalidNodeId { value: String, span: Range<usize> },

    #[error("missing attribute `{attribute}` on `{element}`")]
    MissingAttribute {
        element: String,
        attribute: String,
        span: Range<usize>,
    },

    #[error("invalid temporal value `{value}`")]
    InvalidTemporalValue { value: String, span: Range<usize> },

    #[error("invalid EntityRef `{value}`")]
    InvalidEntityRef { value: String, span: Range<usize> },

    #[error("invalid semantic reference `{value}`")]
    InvalidReference { value: String, span: Range<usize> },

    #[error("unknown Core kind `{value}`")]
    UnknownKind { value: String, span: Range<usize> },

    #[error("invalid Extension metadata: {message}")]
    InvalidExtension { message: String, span: Range<usize> },

    #[error("MDX resource limit exceeded: {resource}")]
    ResourceLimit { resource: &'static str },

    #[error("semantic patch target `{id}` was not found")]
    PatchTargetNotFound { id: String },

    #[error("invalid semantic patch operation: {message}")]
    InvalidPatchOperation { message: String },
}
