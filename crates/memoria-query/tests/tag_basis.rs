use memoria_query::{l2_norm, project_tag_basis};

#[test]
fn orthogonal_basis_reconstructs_query() {
    let q = vec![1.0_f32, 2.0];
    let tags = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
    let r = project_tag_basis(&q, &tags).unwrap();

    assert!(l2_norm(&r.residual) < 1e-5);
    assert_eq!(r.rank, 2);
    assert!(r.used);
    assert!((r.explained_energy - 1.0).abs() < 1e-5);
    assert!(r.conditioning.is_finite());
}

#[test]
fn quality_gate_skips_basis_that_explains_no_query_energy() {
    let q = vec![0.0_f32, 1.0];
    let tags = vec![vec![1.0, 0.0]];
    let r = project_tag_basis(&q, &tags).unwrap();

    assert!(r.skipped());
    assert_eq!(r.residual, q);
    assert_eq!(r.explained, vec![0.0, 0.0]);
    assert_eq!(r.rank, 1);
    assert_eq!(r.explained_energy, 0.0);
}

#[test]
fn mismatched_tag_dimension_is_rejected() {
    let error = project_tag_basis(&[1.0_f32, 2.0], &[vec![1.0]])
        .expect_err("a Tag vector must match the query dimension");

    assert!(matches!(
        error,
        memoria_query::TagBasisError::DimensionMismatch {
            index: 0,
            actual: 1,
            expected: 2,
        }
    ));
}
