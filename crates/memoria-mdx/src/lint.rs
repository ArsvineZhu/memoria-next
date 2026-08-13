use std::ops::Range;

use crate::profile::{SemanticKind, ValidatedDocument};
use crate::syntax::MdxError;

pub const REFERENTIAL_CLOSURE_RISK: &str = "REFERENTIAL_CLOSURE_RISK";
const LINT_VERSION: &str = "referential-closure-v1";
const DEICTIC_TOKENS: &[&str] = &["以后", "那里", "这里", "以前", "后来"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MdxDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Range<usize>,
}

impl MdxDiagnostic {
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        self.span.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LintResult {
    pub document: ValidatedDocument,
    pub diagnostics: Vec<MdxDiagnostic>,
}

impl LintResult {
    #[must_use]
    pub fn source(&self) -> &str {
        self.document.source()
    }

    #[must_use]
    pub fn lint_version(&self) -> &'static str {
        LINT_VERSION
    }
}

pub fn parse_validate_and_lint(source: &str) -> Result<LintResult, MdxError> {
    let document = crate::parse_and_validate(source)?;
    let diagnostics = lint_document(&document);
    Ok(LintResult {
        document,
        diagnostics,
    })
}

pub fn lint_document(document: &ValidatedDocument) -> Vec<MdxDiagnostic> {
    let evidence_spans = document
        .nodes()
        .filter(|node| matches!(node.kind(), SemanticKind::Quote | SemanticKind::Source))
        .map(|node| node.span())
        .collect::<Vec<_>>();
    let mut diagnostics = Vec::new();
    for node in document.nodes() {
        if !is_narrative_kind(node.kind())
            || is_in_evidence_or_contains_evidence(node.span(), &evidence_spans)
        {
            continue;
        }
        for token in DEICTIC_TOKENS {
            if node.text().contains(token) {
                diagnostics.push(MdxDiagnostic {
                    code: REFERENTIAL_CLOSURE_RISK,
                    message: format!(
                        "{LINT_VERSION}: narrative contains unresolved deictic form `{token}`"
                    ),
                    span: node.span(),
                });
            }
        }
    }
    diagnostics
}

fn is_narrative_kind(kind: SemanticKind) -> bool {
    matches!(
        kind,
        SemanticKind::Section | SemanticKind::Entity | SemanticKind::State | SemanticKind::Event
    )
}

fn is_in_evidence_or_contains_evidence(
    span: Range<usize>,
    evidence_spans: &[Range<usize>],
) -> bool {
    evidence_spans.iter().any(|evidence| {
        (evidence.start <= span.start && span.end <= evidence.end)
            || (span.start <= evidence.start && evidence.end <= span.end)
    })
}
