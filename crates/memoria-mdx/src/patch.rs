use std::collections::BTreeMap;

use memoria_types::RevisionSemanticIntent;

use crate::profile::{SemanticKind, SemanticNodeId, ValidatedDocument};
use crate::source::{ParsedSource, SemanticElement};
use crate::syntax::MdxError;
use crate::time::TemporalValue;
use crate::{parse_and_validate, parse_source};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatchOp {
    AddSemanticNode {
        kind: SemanticKind,
        node_id: SemanticNodeId,
        attributes: BTreeMap<String, String>,
        text: String,
    },
    ReplaceSemanticNodeText {
        node_id: SemanticNodeId,
        text: String,
    },
    SetStateValidFrom {
        node_id: SemanticNodeId,
        value: TemporalValue,
    },
    SetStateValidTo {
        node_id: SemanticNodeId,
        value: TemporalValue,
    },
    SetEventOccurredAt {
        node_id: SemanticNodeId,
        value: TemporalValue,
    },
    AddExplicitTag {
        value: String,
    },
    RemoveExplicitTag {
        value: String,
    },
    AddRelation {
        node_id: SemanticNodeId,
        from: SemanticNodeId,
        to: SemanticNodeId,
        kind: String,
    },
    RemoveSemanticNode {
        node_id: SemanticNodeId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionState {
    old_state_id: SemanticNodeId,
    new_state_id: SemanticNodeId,
    effective_from: TemporalValue,
    text: String,
    attributes: BTreeMap<String, String>,
}

impl TransitionState {
    pub fn new(
        old_state_id: SemanticNodeId,
        new_state_id: SemanticNodeId,
        effective_from: TemporalValue,
        text: impl Into<String>,
    ) -> Self {
        Self {
            old_state_id,
            new_state_id,
            effective_from,
            text: text.into(),
            attributes: BTreeMap::new(),
        }
    }

    pub fn with_attributes(mut self, attributes: BTreeMap<String, String>) -> Self {
        self.attributes = attributes;
        self
    }

    #[must_use]
    pub fn semantic_intent(&self) -> RevisionSemanticIntent {
        RevisionSemanticIntent::Transition
    }

    #[must_use]
    pub fn operations(&self) -> Vec<PatchOp> {
        let mut attributes = self.attributes.clone();
        attributes.insert("validFrom".to_owned(), self.effective_from.to_string());
        vec![
            PatchOp::SetStateValidTo {
                node_id: self.old_state_id.clone(),
                value: self.effective_from.clone(),
            },
            PatchOp::AddSemanticNode {
                kind: SemanticKind::State,
                node_id: self.new_state_id.clone(),
                attributes,
                text: self.text.clone(),
            },
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorrectionPatch {
    operations: Vec<PatchOp>,
    semantic_intent: RevisionSemanticIntent,
}

impl CorrectionPatch {
    #[must_use]
    pub fn new(operations: Vec<PatchOp>) -> Self {
        Self {
            operations,
            semantic_intent: RevisionSemanticIntent::Correction,
        }
    }

    #[must_use]
    pub fn semantic_intent(&self) -> RevisionSemanticIntent {
        self.semantic_intent
    }

    #[must_use]
    pub fn operations(&self) -> &[PatchOp] {
        &self.operations
    }

    pub fn apply(&self, source: &str) -> Result<ValidatedDocument, MdxError> {
        apply_patch(source, &self.operations)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupersessionPatch {
    operations: Vec<PatchOp>,
    semantic_intent: RevisionSemanticIntent,
}

impl SupersessionPatch {
    #[must_use]
    pub fn new(operations: Vec<PatchOp>) -> Self {
        Self {
            operations,
            semantic_intent: RevisionSemanticIntent::Supersession,
        }
    }

    #[must_use]
    pub fn semantic_intent(&self) -> RevisionSemanticIntent {
        self.semantic_intent
    }

    #[must_use]
    pub fn operations(&self) -> &[PatchOp] {
        &self.operations
    }

    pub fn apply(&self, source: &str) -> Result<ValidatedDocument, MdxError> {
        apply_patch(source, &self.operations)
    }
}

pub fn apply_patch(source: &str, operations: &[PatchOp]) -> Result<ValidatedDocument, MdxError> {
    let document = parse_and_validate(source)?;
    let parsed = parse_source(source)?;
    let mut edits = Vec::new();
    let mut insertions = BTreeMap::<usize, String>::new();

    for operation in operations {
        match operation {
            PatchOp::AddSemanticNode {
                kind,
                node_id,
                attributes,
                text,
            } => append_fragment(
                source,
                &mut insertions,
                render_node(*kind, node_id, attributes, text)?,
            ),
            PatchOp::ReplaceSemanticNodeText { node_id, text } => {
                let node = target_node(&document, node_id)?;
                let element = target_element(&parsed, node.span())?;
                replace_node_text(source, element, text, &mut edits)?;
            }
            PatchOp::SetStateValidFrom { node_id, value } => {
                let mut workspace = PatchWorkspace {
                    source,
                    document: &document,
                    parsed: &parsed,
                    edits: &mut edits,
                    insertions: &mut insertions,
                };
                set_temporal_attribute(
                    &mut workspace,
                    node_id,
                    SemanticKind::State,
                    "validFrom",
                    value,
                )?;
            }
            PatchOp::SetStateValidTo { node_id, value } => {
                let mut workspace = PatchWorkspace {
                    source,
                    document: &document,
                    parsed: &parsed,
                    edits: &mut edits,
                    insertions: &mut insertions,
                };
                set_temporal_attribute(
                    &mut workspace,
                    node_id,
                    SemanticKind::State,
                    "validTo",
                    value,
                )?;
            }
            PatchOp::SetEventOccurredAt { node_id, value } => {
                let mut workspace = PatchWorkspace {
                    source,
                    document: &document,
                    parsed: &parsed,
                    edits: &mut edits,
                    insertions: &mut insertions,
                };
                set_temporal_attribute(
                    &mut workspace,
                    node_id,
                    SemanticKind::Event,
                    "occurredAt",
                    value,
                )?;
            }
            PatchOp::AddExplicitTag { value } => {
                append_fragment(source, &mut insertions, render_tag(value)?);
            }
            PatchOp::RemoveExplicitTag { value } => {
                let element = parsed
                    .semantic_elements()
                    .find(|element| {
                        element.name() == "Tag"
                            && element.attributes().iter().any(|attribute| {
                                attribute.name() == "value" && attribute.value() == value
                            })
                    })
                    .ok_or_else(|| MdxError::PatchTargetNotFound { id: value.clone() })?;
                edits.push(Edit {
                    range: element.span(),
                    replacement: String::new(),
                });
            }
            PatchOp::AddRelation {
                node_id,
                from,
                to,
                kind,
            } => append_fragment(
                source,
                &mut insertions,
                render_relation(node_id, from, to, kind)?,
            ),
            PatchOp::RemoveSemanticNode { node_id } => {
                let node = target_node(&document, node_id)?;
                edits.push(Edit {
                    range: node.span(),
                    replacement: String::new(),
                });
            }
        }
    }

    for (position, replacement) in insertions {
        edits.push(Edit {
            range: position..position,
            replacement,
        });
    }
    let updated_source = apply_edits(source, edits)?;
    parse_and_validate(&updated_source)
}

pub fn apply_transition_state(
    source: &str,
    transition: &TransitionState,
) -> Result<ValidatedDocument, MdxError> {
    apply_patch(source, &transition.operations())
}

pub fn apply_correction_patch(
    source: &str,
    patch: &CorrectionPatch,
) -> Result<ValidatedDocument, MdxError> {
    apply_patch(source, patch.operations())
}

pub fn apply_supersession_patch(
    source: &str,
    patch: &SupersessionPatch,
) -> Result<ValidatedDocument, MdxError> {
    apply_patch(source, patch.operations())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Edit {
    range: std::ops::Range<usize>,
    replacement: String,
}

struct PatchWorkspace<'a> {
    source: &'a str,
    document: &'a ValidatedDocument,
    parsed: &'a ParsedSource,
    edits: &'a mut Vec<Edit>,
    insertions: &'a mut BTreeMap<usize, String>,
}

fn set_temporal_attribute(
    workspace: &mut PatchWorkspace<'_>,
    node_id: &SemanticNodeId,
    expected_kind: SemanticKind,
    attribute_name: &str,
    value: &TemporalValue,
) -> Result<(), MdxError> {
    let node = target_node(workspace.document, node_id)?;
    if node.kind() != expected_kind {
        return Err(MdxError::InvalidPatchOperation {
            message: format!(
                "`{attribute_name}` can only target {} nodes",
                expected_kind.as_str()
            ),
        });
    }
    let element = target_element(workspace.parsed, node.span())?;
    let value = value.to_string();
    if let Some(attribute) = element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == attribute_name)
    {
        workspace.edits.push(Edit {
            range: attribute.span(),
            replacement: value,
        });
    } else {
        let position = opening_attribute_insertion_point(workspace.source, element)?;
        let fragment = format!(" {attribute_name}=\"{}\"", quote_attribute(&value)?);
        workspace
            .insertions
            .entry(position)
            .or_default()
            .push_str(&fragment);
    }
    Ok(())
}

fn replace_node_text(
    source: &str,
    element: &SemanticElement,
    text: &str,
    edits: &mut Vec<Edit>,
) -> Result<(), MdxError> {
    let open_end = opening_tag_end(source, element)?;
    if element.self_closing() {
        let slash = opening_attribute_insertion_point(source, element)?;
        edits.push(Edit {
            range: slash..element.span().end,
            replacement: format!(">{text}</{}>", element.name()),
        });
        return Ok(());
    }
    let close_start = source[open_end..element.span().end]
        .rfind("</")
        .map(|offset| open_end + offset)
        .ok_or_else(|| MdxError::InvalidPatchOperation {
            message: format!("`{}` has no closing tag", element.name()),
        })?;
    edits.push(Edit {
        range: open_end..close_start,
        replacement: text.to_owned(),
    });
    Ok(())
}

fn target_node<'a>(
    document: &'a ValidatedDocument,
    node_id: &SemanticNodeId,
) -> Result<&'a crate::ValidatedNode, MdxError> {
    document
        .node(node_id)
        .ok_or_else(|| MdxError::PatchTargetNotFound {
            id: node_id.to_string(),
        })
}

fn target_element(
    parsed: &ParsedSource,
    span: std::ops::Range<usize>,
) -> Result<&SemanticElement, MdxError> {
    parsed
        .semantic_elements()
        .find(|element| element.span() == span)
        .ok_or_else(|| MdxError::InvalidPatchOperation {
            message: format!("semantic element span {:?} was not found", span),
        })
}

fn render_node(
    kind: SemanticKind,
    node_id: &SemanticNodeId,
    attributes: &BTreeMap<String, String>,
    text: &str,
) -> Result<String, MdxError> {
    if attributes.contains_key("id") {
        return Err(MdxError::InvalidPatchOperation {
            message: "AddSemanticNode attributes must not contain `id`".to_owned(),
        });
    }
    let mut output = format!("<{} id=\"{}\"", kind.as_str(), node_id);
    for (name, value) in attributes {
        if name.is_empty() || name == "id" {
            return Err(MdxError::InvalidPatchOperation {
                message: "semantic patch attribute name is invalid".to_owned(),
            });
        }
        output.push(' ');
        output.push_str(name);
        output.push_str("=\"");
        output.push_str(&quote_attribute(value)?);
        output.push('"');
    }
    if text.is_empty() {
        output.push_str("/>");
    } else {
        output.push('>');
        output.push_str(text);
        output.push_str("</");
        output.push_str(kind.as_str());
        output.push('>');
    }
    Ok(output)
}

fn render_tag(value: &str) -> Result<String, MdxError> {
    Ok(format!("<Tag value=\"{}\"/>", quote_attribute(value)?))
}

fn render_relation(
    node_id: &SemanticNodeId,
    from: &SemanticNodeId,
    to: &SemanticNodeId,
    kind: &str,
) -> Result<String, MdxError> {
    Ok(format!(
        "<Relation id=\"{node_id}\" from=\"#{from}\" to=\"#{to}\" kind=\"{}\"/>",
        quote_attribute(kind)?
    ))
}

fn quote_attribute(value: &str) -> Result<String, MdxError> {
    if value
        .chars()
        .any(|character| matches!(character, '"' | '\'' | '{' | '}' | '\r' | '\n'))
    {
        return Err(MdxError::InvalidPatchOperation {
            message: "semantic patch attribute contains a forbidden character".to_owned(),
        });
    }
    Ok(value.to_owned())
}

fn append_fragment(source: &str, insertions: &mut BTreeMap<usize, String>, fragment: String) {
    let entry = insertions.entry(source.len()).or_default();
    if entry.is_empty() && !source.is_empty() && !source.ends_with('\n') {
        entry.push('\n');
    }
    if !entry.is_empty() && !entry.ends_with('\n') {
        entry.push('\n');
    }
    entry.push_str(&fragment);
    entry.push('\n');
}

fn apply_edits(source: &str, mut edits: Vec<Edit>) -> Result<String, MdxError> {
    for (index, left) in edits.iter().enumerate() {
        for right in edits.iter().skip(index + 1) {
            if edits_conflict(left, right) {
                return Err(MdxError::InvalidPatchOperation {
                    message: "semantic patch operations overlap".to_owned(),
                });
            }
        }
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    let mut output = source.to_owned();
    for edit in edits {
        output.replace_range(edit.range, &edit.replacement);
    }
    Ok(output)
}

fn edits_conflict(left: &Edit, right: &Edit) -> bool {
    let left_empty = left.range.start == left.range.end;
    let right_empty = right.range.start == right.range.end;
    if left_empty && right_empty {
        return left.range.start == right.range.start;
    }
    if left_empty {
        return right.range.start < left.range.start && left.range.start < right.range.end;
    }
    if right_empty {
        return left.range.start < right.range.start && right.range.start < left.range.end;
    }
    left.range.start < right.range.end && right.range.start < left.range.end
}

fn opening_tag_end(source: &str, element: &SemanticElement) -> Result<usize, MdxError> {
    let span = element.span();
    let bytes = source.as_bytes();
    let mut cursor = span.start + 1;
    let mut quote = None;
    while cursor < span.end {
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
    Err(MdxError::InvalidPatchOperation {
        message: format!("`{}` opening tag has no closing bracket", element.name()),
    })
}

fn opening_attribute_insertion_point(
    source: &str,
    element: &SemanticElement,
) -> Result<usize, MdxError> {
    let end = opening_tag_end(source, element)?;
    let mut cursor = end - 1;
    if element.self_closing() {
        while cursor > element.span().start && source.as_bytes()[cursor - 1].is_ascii_whitespace() {
            cursor -= 1;
        }
        if cursor > element.span().start && source.as_bytes()[cursor - 1] == b'/' {
            return Ok(cursor - 1);
        }
    }
    Ok(end - 1)
}
