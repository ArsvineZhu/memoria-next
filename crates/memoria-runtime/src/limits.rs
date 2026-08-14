#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub max_source_bytes: usize,
    pub max_query_results: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_query_results: 1_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimitError {
    pub resource: &'static str,
    pub actual: usize,
    pub maximum: usize,
}

pub fn check_source_bytes(
    limits: ResourceLimits,
    source_bytes: usize,
) -> Result<(), ResourceLimitError> {
    if source_bytes > limits.max_source_bytes {
        Err(ResourceLimitError {
            resource: "source_bytes",
            actual: source_bytes,
            maximum: limits.max_source_bytes,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ResourceLimits, check_source_bytes};

    #[test]
    fn source_limit_rejects_before_work() {
        let limits = ResourceLimits {
            max_source_bytes: 4,
            max_query_results: 10,
        };
        assert!(check_source_bytes(limits, 5).is_err());
        assert!(check_source_bytes(limits, 4).is_ok());
    }
}
