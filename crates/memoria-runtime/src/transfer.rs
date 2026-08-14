use std::collections::{BTreeMap, BTreeSet};

use memoria_mdx::{MdxError, parse_and_validate, parse_source};
use memoria_types::{MemoryId, RevisionId, SpaceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableMemory {
    pub source_id: MemoryId,
    pub space_id: SpaceId,
    pub revision_id: RevisionId,
    pub mdx: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableImportRequest {
    pub target_space_key: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
    pub origin_store_id: Option<String>,
    pub memories: Vec<PortableMemory>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceRewrite {
    pub rewritten: Vec<String>,
    pub unresolved: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceReferenceRewrite {
    pub rewritten: String,
    pub unresolved: Vec<String>,
}

#[must_use]
pub fn rewrite_reference_ids(
    references: &[String],
    mappings: &BTreeMap<String, String>,
) -> ReferenceRewrite {
    let mut unresolved = BTreeSet::new();
    let rewritten = references
        .iter()
        .map(|reference| {
            mappings.get(reference).cloned().unwrap_or_else(|| {
                unresolved.insert(reference.clone());
                reference.clone()
            })
        })
        .collect();
    ReferenceRewrite {
        rewritten,
        unresolved: unresolved.into_iter().collect(),
    }
}

/// Rewrite only structured `MemoryRef` attributes in a parsed source.
///
/// Text, comments, fenced code, and unrelated attributes are intentionally
/// left untouched. Missing mappings are retained verbatim and returned as
/// unresolved external references.
pub fn rewrite_memory_ref_source(
    source: &str,
    mappings: &BTreeMap<String, String>,
) -> Result<SourceReferenceRewrite, MdxError> {
    let parsed = parse_source(source)?;
    let mut replacements = Vec::new();
    let mut unresolved = BTreeSet::new();
    for element in parsed.semantic_elements() {
        if element.name() != "MemoryRef" {
            continue;
        }
        for attribute in element
            .attributes()
            .iter()
            .filter(|attribute| matches!(attribute.name(), "memoryId" | "memory_id" | "ref"))
        {
            let value = attribute.value();
            if let Some(target) = mappings.get(value) {
                replacements.push((attribute.span(), target.clone()));
            } else if !value.is_empty() {
                unresolved.insert(value.to_owned());
            }
        }
    }
    replacements.sort_by_key(|(span, _)| span.start);
    let mut rewritten = source.to_owned();
    for (span, target) in replacements.into_iter().rev() {
        rewritten.replace_range(span, &target);
    }
    parse_and_validate(&rewritten)?;
    Ok(SourceReferenceRewrite {
        rewritten,
        unresolved: unresolved.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{rewrite_memory_ref_source, rewrite_reference_ids};
    use std::collections::BTreeMap;

    #[test]
    fn package_internal_references_are_remapped_and_external_are_reported() {
        let mappings = BTreeMap::from([("M_source".to_owned(), "M_target".to_owned())]);
        let result =
            rewrite_reference_ids(&["M_source".to_owned(), "M_external".to_owned()], &mappings);

        assert_eq!(result.rewritten, ["M_target", "M_external"]);
        assert_eq!(result.unresolved, ["M_external"]);
    }

    #[test]
    fn source_rewrite_changes_only_memory_ref_attributes() {
        let mappings = BTreeMap::from([("M_source".to_owned(), "M_target".to_owned())]);
        let source = r#"# M_source

<!-- <MemoryRef memoryId="M_source"/> -->

```md
<MemoryRef memoryId="M_source"/>
```

<MemoryRef memoryId="M_source" ref="M_external"/>"#;
        let result = rewrite_memory_ref_source(source, &mappings).unwrap();
        assert!(result.rewritten.contains("# M_source"));
        assert!(result.rewritten.contains("memoryId=\"M_target\""));
        assert!(result.rewritten.contains("ref=\"M_external\""));
        assert!(
            result
                .rewritten
                .contains("<!-- <MemoryRef memoryId=\"M_source\"/> -->")
        );
        assert!(
            result
                .rewritten
                .contains("<MemoryRef memoryId=\"M_source\"/>\n```")
        );
        assert_eq!(result.unresolved, ["M_external"]);
    }

    #[test]
    fn source_rewrite_revalidates_all_rewritten_documents() {
        let mappings = BTreeMap::from([("M_source".to_owned(), "M_target".to_owned())]);
        let error =
            rewrite_memory_ref_source(r#"<Entity ref="person:ada">unterminated"#, &mappings)
                .unwrap_err();
        assert!(matches!(
            error,
            memoria_mdx::MdxError::UnclosedElement { .. }
        ));
    }
}
