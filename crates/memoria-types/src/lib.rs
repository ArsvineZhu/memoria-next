mod digest;
mod entity_ref;
mod error;
mod id;
mod snapshot;
mod time;

pub use digest::SourceBlobHash;
pub use entity_ref::{EntityRef, EntityRefError};
pub use error::MemoriaError;
pub use id::{MemoryId, RevisionId, SpaceId, StoreId};
pub use snapshot::{
    AdaptiveGeneration, AdaptiveSnapshot, AuthorityGeneration, AuthoritySnapshot,
    RevisionSemanticIntent,
};
pub use time::{Timestamp, UnixTimestamp, UtcTimestamp};

#[cfg(test)]
mod tests {
    use super::{MemoryId, SpaceId};

    #[test]
    fn memory_ids_round_trip_with_type_prefix() {
        let id = MemoryId::new();
        let text = id.to_string();
        assert!(text.starts_with("M_"));
        assert_eq!(text.parse::<MemoryId>().unwrap(), id);
    }

    #[test]
    fn memory_id_cannot_parse_as_space_id() {
        let text = MemoryId::new().to_string();
        assert!(text.parse::<SpaceId>().is_err());
    }
}
