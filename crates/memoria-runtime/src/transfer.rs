use std::collections::{BTreeMap, BTreeSet};

use memoria_types::{MemoryId, RevisionId, SpaceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableMemory {
    pub source_id: MemoryId,
    pub space_id: SpaceId,
    pub revision_id: RevisionId,
    pub mdx: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceRewrite {
    pub rewritten: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::rewrite_reference_ids;
    use std::collections::BTreeMap;

    #[test]
    fn package_internal_references_are_remapped_and_external_are_reported() {
        let mappings = BTreeMap::from([("M_source".to_owned(), "M_target".to_owned())]);
        let result =
            rewrite_reference_ids(&["M_source".to_owned(), "M_external".to_owned()], &mappings);

        assert_eq!(result.rewritten, ["M_target", "M_external"]);
        assert_eq!(result.unresolved, ["M_external"]);
    }
}
