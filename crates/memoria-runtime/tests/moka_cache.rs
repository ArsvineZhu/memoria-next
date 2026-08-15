use std::sync::Arc;
use std::time::Duration;

use memoria_derived::DerivedManifest;
use memoria_query::AssociationGraphView;
use memoria_runtime::{AssociationCacheKey, QueryEmbeddingCacheKey, RuntimeCaches};
use memoria_types::AuthorityGeneration;

#[test]
fn query_operation_expires_after_five_minutes() {
    assert_eq!(
        RuntimeCaches::query_operation_ttl(),
        Duration::from_secs(5 * 60)
    );
    assert_eq!(
        RuntimeCaches::new().query_operation_policy().time_to_live(),
        Some(Duration::from_secs(5 * 60))
    );
}

#[test]
fn query_operation_capacity_is_bounded() {
    assert_eq!(RuntimeCaches::query_operation_max_capacity(), 1024);
    assert_eq!(
        RuntimeCaches::new().query_operation_policy().max_capacity(),
        Some(1024)
    );
}

#[test]
fn query_embedding_cache_is_bounded_to_256_entries() {
    let caches = RuntimeCaches::new();
    let manifest = DerivedManifest::empty_for_lexical_query(AuthorityGeneration::new(1));
    for index in 0_u8..=255 {
        let key = QueryEmbeddingCacheKey::new(
            manifest.authority_generation(),
            manifest.id(),
            [index; 32],
        );
        caches.insert_query_embedding(key, Arc::new(vec![f32::from(index)]));
    }
    caches.run_pending_tasks();

    assert!(caches.query_embedding_entry_count() <= 256);
    let key =
        QueryEmbeddingCacheKey::new(manifest.authority_generation(), manifest.id(), [255; 32]);
    assert_eq!(
        caches
            .get_query_embedding(&key)
            .map(|value| value.as_ref().clone()),
        Some(vec![255.0])
    );
}

#[test]
fn manifest_change_invalidates_retrieval_view_cache_key_naturally() {
    let caches = RuntimeCaches::new();
    let old_manifest = DerivedManifest::empty_for_lexical_query(AuthorityGeneration::new(1));
    let new_manifest = DerivedManifest::empty_for_lexical_query(AuthorityGeneration::new(2));
    let old_key = AssociationCacheKey::new(
        old_manifest.authority_generation(),
        old_manifest.id(),
        [7; 32],
    );
    let new_key = AssociationCacheKey::new(
        new_manifest.authority_generation(),
        new_manifest.id(),
        [7; 32],
    );

    caches.insert_association_view(old_key.clone(), Arc::new(AssociationGraphView::new()));
    caches.run_pending_tasks();

    assert!(caches.get_association_view(&old_key).is_some());
    assert!(caches.get_association_view(&new_key).is_none());
}
