mod canonical;
mod diff;
mod ir;
mod lint;
mod mdast_adapter;
mod patch;
mod profile;
mod semantic_lexer;
mod source;
mod syntax;
mod time;
mod validate;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use canonical::{compile_ir, compile_validated};
pub use diff::{InvalidationCategory, SemanticDiff, SemanticDiffCategory};
pub use ir::{IrNode, MEMORY_IR_VERSION, MemoryIr, SemanticHash, SourceMapping};
pub use lint::{
    LintResult, MdxDiagnostic, REFERENTIAL_CLOSURE_RISK, lint_document, parse_validate_and_lint,
};
pub use mdast_adapter::{mdast_source_span, parse_mdast};
pub use memoria_types::RevisionSemanticIntent;
pub use patch::{
    CorrectionPatch, PatchOp, SupersessionPatch, TransitionState, apply_correction_patch,
    apply_patch, apply_supersession_patch, apply_transition_state,
};
pub use profile::{NodeId, SemanticKind, SemanticNodeId, ValidatedDocument, ValidatedNode};
pub use source::{ParsedSource, SemanticAttribute, SemanticElement};
pub use syntax::MdxError;
pub use time::{TemporalPrecision, TemporalValue};
pub use validate::{parse_and_validate, validate_parsed};

pub fn parse_source(source: &str) -> Result<ParsedSource, MdxError> {
    mdast_adapter::parse_source(source)
}
