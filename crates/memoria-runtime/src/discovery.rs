use std::collections::BTreeSet;

use memoria_types::{MemoryId, SpaceId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityObservation {
    pub entity_ref: String,
    pub surface: String,
    pub space_id: SpaceId,
    pub memory_id: Option<MemoryId>,
}

/// Filter observations at the source boundary, before any count or ranking is
/// derived. This prevents unauthorized Spaces from influencing disclosure.
#[must_use]
pub fn discover_scoped(
    observations: &[EntityObservation],
    surface: &str,
    allowed_spaces: &[SpaceId],
) -> Vec<EntityObservation> {
    let allowed = allowed_spaces.iter().copied().collect::<BTreeSet<_>>();
    observations
        .iter()
        .filter(|observation| {
            observation.surface == surface && allowed.contains(&observation.space_id)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{EntityObservation, discover_scoped};
    use memoria_types::SpaceId;

    #[test]
    fn discovery_filters_before_result_construction() {
        let public = SpaceId::from_bytes([1; 16]);
        let secret = SpaceId::from_bytes([2; 16]);
        let observations = vec![
            EntityObservation {
                entity_ref: "person:alex".to_owned(),
                surface: "Alex".to_owned(),
                space_id: public,
                memory_id: None,
            },
            EntityObservation {
                entity_ref: "person:alex-secret".to_owned(),
                surface: "Alex".to_owned(),
                space_id: secret,
                memory_id: None,
            },
        ];

        let result = discover_scoped(&observations, "Alex", &[public]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].space_id, public);
        assert!(!format!("{result:?}").contains("alex-secret"));
    }
}
