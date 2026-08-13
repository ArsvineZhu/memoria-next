use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{IrNode, MemoryIr};
use crate::profile::SemanticKind;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InvalidationCategory {
    VisibleText,
    Hierarchy,
    EntityBinding,
    EntitySurface,
    ExplicitTag,
    Temporal,
    Relation,
    MemoryRef,
    SourceMetadata,
    Extension,
    SemanticNodeLifecycle,
}

pub type SemanticDiffCategory = InvalidationCategory;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SemanticDiff {
    per_node: BTreeMap<String, BTreeSet<InvalidationCategory>>,
    global: BTreeSet<InvalidationCategory>,
}

impl SemanticDiff {
    #[must_use]
    pub fn between(old: &MemoryIr, new: &MemoryIr) -> Self {
        let old_by_id = old
            .nodes()
            .filter_map(|node| node.id().map(|id| (id.to_string(), node)))
            .collect::<BTreeMap<_, _>>();
        let new_by_id = new
            .nodes()
            .filter_map(|node| node.id().map(|id| (id.to_string(), node)))
            .collect::<BTreeMap<_, _>>();
        let mut diff = Self::default();
        let ids = old_by_id
            .keys()
            .chain(new_by_id.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        for id in ids {
            match (old_by_id.get(&id), new_by_id.get(&id)) {
                (Some(old_node), Some(new_node)) => {
                    let categories = compare_nodes(old_node, new_node);
                    if !categories.is_empty() {
                        diff.per_node.insert(id, categories);
                    }
                }
                (Some(old_node), None) | (None, Some(old_node)) => {
                    let mut categories = BTreeSet::new();
                    categories.insert(InvalidationCategory::SemanticNodeLifecycle);
                    categories.extend(kind_categories(old_node.kind()));
                    diff.per_node.insert(id, categories);
                }
                (None, None) => unreachable!("union of node ID maps contains an absent key"),
            }
        }

        compare_unidentified_nodes(old, new, &mut diff.global);
        diff
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty() && self.global.is_empty()
    }

    #[must_use]
    pub fn has_category(&self, category: InvalidationCategory) -> bool {
        self.global.contains(&category)
            || self
                .per_node
                .values()
                .any(|categories| categories.contains(&category))
    }

    #[must_use]
    pub fn category_changed(&self, id: &str, category: InvalidationCategory) -> bool {
        self.per_node
            .get(id)
            .is_some_and(|categories| categories.contains(&category))
    }

    #[must_use]
    pub fn temporal_changed(&self, id: &str) -> bool {
        self.category_changed(id, InvalidationCategory::Temporal)
    }

    #[must_use]
    pub fn visible_text_changed(&self, id: &str) -> bool {
        self.category_changed(id, InvalidationCategory::VisibleText)
    }

    pub fn categories_for(&self, id: &str) -> impl Iterator<Item = InvalidationCategory> + '_ {
        self.per_node
            .get(id)
            .into_iter()
            .flat_map(|categories| categories.iter().copied())
    }
}

fn compare_nodes(old: &IrNode, new: &IrNode) -> BTreeSet<InvalidationCategory> {
    let mut categories = BTreeSet::new();
    if old.kind() != new.kind() {
        categories.insert(InvalidationCategory::SemanticNodeLifecycle);
        return categories;
    }
    if old.text() != new.text() {
        if old.kind() == SemanticKind::Extension {
            categories.insert(InvalidationCategory::Extension);
        } else {
            categories.insert(InvalidationCategory::VisibleText);
            if old.kind() == SemanticKind::Entity {
                categories.insert(InvalidationCategory::EntitySurface);
            }
        }
    }
    if old.occurred_at() != new.occurred_at()
        || old.observed_at() != new.observed_at()
        || old.valid_from() != new.valid_from()
        || old.valid_to() != new.valid_to()
    {
        categories.insert(InvalidationCategory::Temporal);
    }
    if non_temporal_attributes(old) != non_temporal_attributes(new) {
        categories.extend(attribute_categories(old.kind(), old, new));
    }
    categories
}

fn non_temporal_attributes(node: &IrNode) -> Vec<(&str, &str)> {
    node.attributes()
        .iter()
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "occurredAt" | "observedAt" | "validFrom" | "validTo"
            )
        })
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect()
}

fn attribute_categories(
    kind: SemanticKind,
    old: &IrNode,
    new: &IrNode,
) -> BTreeSet<InvalidationCategory> {
    let mut categories = BTreeSet::new();
    match kind {
        SemanticKind::Entity => {
            if attribute_changed(old, new, "ref") || attribute_changed(old, new, "about") {
                categories.insert(InvalidationCategory::EntityBinding);
            } else {
                categories.insert(InvalidationCategory::EntitySurface);
            }
        }
        SemanticKind::Tag => {
            categories.insert(InvalidationCategory::ExplicitTag);
        }
        SemanticKind::Relation => {
            categories.insert(InvalidationCategory::Relation);
        }
        SemanticKind::MemoryRef => {
            categories.insert(InvalidationCategory::MemoryRef);
        }
        SemanticKind::Source => {
            categories.insert(InvalidationCategory::SourceMetadata);
        }
        SemanticKind::Quote => {
            if attribute_changed(old, new, "speaker") || attribute_changed(old, new, "source") {
                categories.insert(InvalidationCategory::SourceMetadata);
            } else {
                categories.insert(InvalidationCategory::VisibleText);
            }
        }
        SemanticKind::Extension => {
            categories.insert(InvalidationCategory::Extension);
        }
        SemanticKind::Section | SemanticKind::MemoryMeta => {
            categories.insert(InvalidationCategory::Hierarchy);
        }
        SemanticKind::State | SemanticKind::Event => {
            if attribute_changed(old, new, "about") {
                categories.insert(InvalidationCategory::EntityBinding);
            } else {
                categories.insert(InvalidationCategory::Hierarchy);
            }
        }
    }
    categories
}

fn attribute_changed(old: &IrNode, new: &IrNode, name: &str) -> bool {
    attribute(old, name) != attribute(new, name)
}

fn attribute<'a>(node: &'a IrNode, name: &str) -> Option<&'a str> {
    node.attributes()
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}

fn compare_unidentified_nodes(
    old: &MemoryIr,
    new: &MemoryIr,
    global: &mut BTreeSet<InvalidationCategory>,
) {
    let old_nodes = old
        .nodes()
        .filter(|node| node.id().is_none())
        .collect::<Vec<_>>();
    let new_nodes = new
        .nodes()
        .filter(|node| node.id().is_none())
        .collect::<Vec<_>>();
    let mut matched_new = vec![false; new_nodes.len()];
    for old_node in old_nodes {
        if let Some((index, new_node)) = new_nodes
            .iter()
            .enumerate()
            .find(|(index, node)| !matched_new[*index] && same_semantics(old_node, node))
        {
            matched_new[index] = true;
            let categories = compare_nodes(old_node, new_node);
            global.extend(categories);
        } else {
            global.extend(kind_categories(old_node.kind()));
        }
    }
    for (index, node) in new_nodes.iter().enumerate() {
        if !matched_new[index] {
            global.extend(kind_categories(node.kind()));
        }
    }
}

fn same_semantics(left: &IrNode, right: &IrNode) -> bool {
    left.kind() == right.kind()
        && left.attributes() == right.attributes()
        && left.text() == right.text()
        && left.occurred_at() == right.occurred_at()
        && left.observed_at() == right.observed_at()
        && left.valid_from() == right.valid_from()
        && left.valid_to() == right.valid_to()
}

fn kind_categories(kind: SemanticKind) -> BTreeSet<InvalidationCategory> {
    let mut categories = BTreeSet::new();
    categories.insert(InvalidationCategory::SemanticNodeLifecycle);
    match kind {
        SemanticKind::Entity => {
            categories.insert(InvalidationCategory::EntityBinding);
            categories.insert(InvalidationCategory::EntitySurface);
        }
        SemanticKind::Tag => {
            categories.insert(InvalidationCategory::ExplicitTag);
        }
        SemanticKind::Relation => {
            categories.insert(InvalidationCategory::Relation);
        }
        SemanticKind::MemoryRef => {
            categories.insert(InvalidationCategory::MemoryRef);
        }
        SemanticKind::Source | SemanticKind::Quote => {
            categories.insert(InvalidationCategory::SourceMetadata);
        }
        SemanticKind::Extension => {
            categories.insert(InvalidationCategory::Extension);
        }
        SemanticKind::Section | SemanticKind::MemoryMeta => {
            categories.insert(InvalidationCategory::Hierarchy);
        }
        SemanticKind::State | SemanticKind::Event => {
            categories.insert(InvalidationCategory::Temporal);
        }
    }
    categories
}
