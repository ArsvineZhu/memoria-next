use memoria_derived::TagId;
use memoria_query::{
    SemanticResolution, SupportEvidence, TagSeedProvenance, correlated_resolutions,
    structure_bonus, support_bonus,
};

fn support(independent_seed_count: usize, strongest_support: f32) -> SupportEvidence {
    SupportEvidence {
        tag_id: TagId::from_bytes([1; 32]),
        score: 1.0,
        seed_origins: vec![TagSeedProvenance::Explicit],
        strongest_support,
        independent_seed_count,
        static_contribution: 1.0,
        adaptive_contribution: 0.0,
    }
}

#[test]
fn duplicate_paths_from_same_seed_do_not_increase_independent_support() {
    let one_path = support(1, 0.5);
    let duplicate_path = support(1, 0.5);
    assert_eq!(
        one_path.independent_seed_count,
        duplicate_path.independent_seed_count
    );
    assert_eq!(support_bonus(&one_path), support_bonus(&duplicate_path));
}

#[test]
fn independent_seed_support_uses_log_saturation() {
    let one = support(1, 1.0);
    let four = support(4, 1.0);
    assert!(support_bonus(&four) > support_bonus(&one));
    assert!(support_bonus(&four) <= 0.12);
}

#[test]
fn parent_and_child_hits_are_marked_correlated() {
    assert!(correlated_resolutions(&[
        SemanticResolution::Leaf,
        SemanticResolution::Node,
    ]));
    assert!(!correlated_resolutions(&[SemanticResolution::Leaf]));
}

#[test]
fn support_bonus_never_exceeds_0_12() {
    assert!(support_bonus(&support(usize::MAX, 100.0)) <= 0.12);
}

#[test]
fn structure_bonus_never_exceeds_0_08() {
    assert!(structure_bonus(100.0, 0, true) <= 0.08);
}
