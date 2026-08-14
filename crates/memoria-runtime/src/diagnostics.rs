use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedactedDiagnostic {
    pub label: String,
    pub sha256: String,
    pub byte_count: usize,
}

#[must_use]
pub fn redact_text(label: impl Into<String>, value: &[u8]) -> RedactedDiagnostic {
    let mut hasher = Sha256::new();
    hasher.update(value);
    RedactedDiagnostic {
        label: label.into(),
        sha256: hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        byte_count: value.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::redact_text;

    #[test]
    fn diagnostics_keep_shape_without_raw_content() {
        let diagnostic = redact_text("mdx", b"private source");
        assert_eq!(diagnostic.byte_count, 14);
        assert!(!format!("{diagnostic:?}").contains("private source"));
    }
}
