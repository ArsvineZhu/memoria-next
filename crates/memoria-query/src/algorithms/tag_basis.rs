use nalgebra::{DMatrix, DVector};
use thiserror::Error;

/// Maximum number of dimensions accepted by the bounded baseline.
pub const MAX_TAG_BASIS_DIMENSIONS: usize = 4096;

/// Maximum number of Tag vectors accepted by the bounded baseline.
pub const MAX_TAG_BASIS_VECTORS: usize = 64;

const CONDITIONING_LIMIT: f64 = 1.0e6;
const MIN_EXPLAINED_ENERGY: f64 = 1.0e-6;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TagBasisError {
    #[error("Tag Basis query vector must not be empty")]
    EmptyQuery,

    #[error("Tag Basis requires at least one Tag vector")]
    EmptyBasis,

    #[error("Tag Basis dimension {actual} exceeds the limit {limit}")]
    DimensionLimit { actual: usize, limit: usize },

    #[error("Tag Basis vector count {actual} exceeds the limit {limit}")]
    VectorLimit { actual: usize, limit: usize },

    #[error("Tag Basis vector {index} has dimension {actual}; expected {expected}")]
    DimensionMismatch {
        index: usize,
        actual: usize,
        expected: usize,
    },

    #[error("Tag Basis input contains a non-finite value")]
    NonFinite,

    #[error("Tag Basis SVD did not produce left singular vectors")]
    MissingLeftSingularVectors,
}

/// The bounded numerical result used by Tag-aware semantic retrieval.
#[derive(Clone, Debug, PartialEq)]
pub struct TagBasisResult {
    /// The component of the query explained by the accepted Tag basis.
    pub explained: Vec<f32>,
    /// The residual left after subtracting `explained` from the query.
    pub residual: Vec<f32>,
    /// Numerical rank of the supplied Tag vectors.
    pub rank: usize,
    /// Ratio of the largest to smallest retained singular value.
    pub conditioning: f32,
    /// Fraction of query energy explained by the raw SVD projection.
    pub explained_energy: f32,
    /// Whether the quality gate accepted the basis projection.
    pub used: bool,
}

impl TagBasisResult {
    #[must_use]
    pub fn skipped(&self) -> bool {
        !self.used
    }
}

/// Compute the Euclidean norm without exposing the numerical backend.
#[must_use]
pub fn l2_norm(values: &[f32]) -> f32 {
    values
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt() as f32
}

/// Project a query onto the column space spanned by bounded Tag vectors.
///
/// The SVD is deliberately performed in `f64`, even though the public vectors
/// are `f32`, to make rank and residual diagnostics stable for small bases.
/// A valid but ill-conditioned or uninformative basis is returned with
/// `used == false`; in that case `residual` is the original query and
/// `explained` is zero, so callers can safely skip the basis contribution.
pub fn project_tag_basis(
    query: &[f32],
    tags: &[Vec<f32>],
) -> Result<TagBasisResult, TagBasisError> {
    validate_inputs(query, tags)?;

    let dimension = query.len();
    let vector_count = tags.len();
    let matrix = DMatrix::<f64>::from_fn(dimension, vector_count, |row, column| {
        f64::from(tags[column][row])
    });
    let query_vector =
        DVector::<f64>::from_iterator(dimension, query.iter().copied().map(f64::from));
    let svd = matrix.svd(true, false);
    let singular_values = svd.singular_values.as_slice();
    let Some(maximum_singular_value) = singular_values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .max_by(f64::total_cmp)
    else {
        return Ok(skipped_result(query, 0, f32::INFINITY, 0.0));
    };

    let tolerance =
        maximum_singular_value * (dimension.max(vector_count) as f64) * f64::from(f32::EPSILON);
    let rank = singular_values
        .iter()
        .filter(|value| value.is_finite() && **value > tolerance)
        .count();
    let minimum_retained_singular_value = singular_values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > tolerance)
        .min_by(f64::total_cmp);
    let conditioning = minimum_retained_singular_value
        .map(|minimum| maximum_singular_value / minimum)
        .unwrap_or(f64::INFINITY);

    let Some(left_singular_vectors) = svd.u else {
        return Err(TagBasisError::MissingLeftSingularVectors);
    };
    let mut raw_explained = DVector::<f64>::zeros(dimension);
    for column in 0..rank {
        let basis = left_singular_vectors.column(column);
        let coefficient = basis.dot(&query_vector);
        for row in 0..dimension {
            raw_explained[row] += basis[row] * coefficient;
        }
    }
    let raw_residual = &query_vector - &raw_explained;
    let query_energy = query_vector.norm_squared();
    let explained_energy = if query_energy > 0.0 {
        (raw_explained.norm_squared() / query_energy).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let used = rank > 0
        && conditioning.is_finite()
        && conditioning <= CONDITIONING_LIMIT
        && explained_energy >= MIN_EXPLAINED_ENERGY;

    if used {
        Ok(TagBasisResult {
            explained: to_f32(&raw_explained),
            residual: to_f32(&raw_residual),
            rank,
            conditioning: conditioning as f32,
            explained_energy: explained_energy as f32,
            used,
        })
    } else {
        Ok(skipped_result(
            query,
            rank,
            conditioning as f32,
            explained_energy as f32,
        ))
    }
}

fn validate_inputs(query: &[f32], tags: &[Vec<f32>]) -> Result<(), TagBasisError> {
    if query.is_empty() {
        return Err(TagBasisError::EmptyQuery);
    }
    if tags.is_empty() {
        return Err(TagBasisError::EmptyBasis);
    }
    if query.len() > MAX_TAG_BASIS_DIMENSIONS {
        return Err(TagBasisError::DimensionLimit {
            actual: query.len(),
            limit: MAX_TAG_BASIS_DIMENSIONS,
        });
    }
    if tags.len() > MAX_TAG_BASIS_VECTORS {
        return Err(TagBasisError::VectorLimit {
            actual: tags.len(),
            limit: MAX_TAG_BASIS_VECTORS,
        });
    }
    if query.iter().any(|value| !value.is_finite()) {
        return Err(TagBasisError::NonFinite);
    }
    for (index, tag) in tags.iter().enumerate() {
        if tag.len() != query.len() {
            return Err(TagBasisError::DimensionMismatch {
                index,
                actual: tag.len(),
                expected: query.len(),
            });
        }
        if tag.iter().any(|value| !value.is_finite()) {
            return Err(TagBasisError::NonFinite);
        }
    }
    Ok(())
}

fn skipped_result(
    query: &[f32],
    rank: usize,
    conditioning: f32,
    explained_energy: f32,
) -> TagBasisResult {
    TagBasisResult {
        explained: vec![0.0; query.len()],
        residual: query.to_vec(),
        rank,
        conditioning,
        explained_energy,
        used: false,
    }
}

fn to_f32(values: &DVector<f64>) -> Vec<f32> {
    values.iter().map(|value| *value as f32).collect()
}
