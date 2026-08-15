use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticAttribute {
    name: String,
    value: String,
    span: Range<usize>,
}

impl SemanticAttribute {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticElement {
    name: String,
    span: Range<usize>,
    attributes: Vec<SemanticAttribute>,
    self_closing: bool,
    text: String,
}

impl SemanticElement {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }

    #[must_use]
    pub fn attributes(&self) -> &[SemanticAttribute] {
        &self.attributes
    }

    #[must_use]
    pub fn self_closing(&self) -> bool {
        self.self_closing
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedSource {
    source: String,
    elements: Vec<SemanticElement>,
}

impl ParsedSource {
    pub(crate) fn new(source: String, elements: Vec<SemanticElement>) -> Self {
        Self { source, elements }
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn semantic_elements(&self) -> impl Iterator<Item = &SemanticElement> {
        self.elements.iter()
    }
}

pub(crate) fn make_attribute(name: String, value: String, span: Range<usize>) -> SemanticAttribute {
    SemanticAttribute { name, value, span }
}

pub(crate) fn make_element(
    name: String,
    span: Range<usize>,
    attributes: Vec<SemanticAttribute>,
    self_closing: bool,
) -> SemanticElement {
    make_element_with_text(name, span, attributes, self_closing, String::new())
}

pub(crate) fn make_element_with_text(
    name: String,
    span: Range<usize>,
    attributes: Vec<SemanticAttribute>,
    self_closing: bool,
    text: String,
) -> SemanticElement {
    SemanticElement {
        name,
        span,
        attributes,
        self_closing,
        text,
    }
}
