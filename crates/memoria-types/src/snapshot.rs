use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{MemoriaError, Timestamp};

macro_rules! define_generation {
    ($name:ident, $kind:literal) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            Deserialize,
        )]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn initial() -> Self {
                Self(0)
            }

            #[must_use]
            pub const fn value(self) -> u64 {
                self.0
            }

            #[must_use]
            pub const fn checked_next(self) -> Option<Self> {
                match self.0.checked_add(1) {
                    Some(value) => Some(Self(value)),
                    None => None,
                }
            }

            pub const fn next(self) -> Self {
                Self(self.0.saturating_add(1))
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self::new(value)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.value()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = MemoriaError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value
                    .parse::<u64>()
                    .map(Self::new)
                    .map_err(|_| MemoriaError::InvalidGeneration {
                        kind: $kind,
                        value: value.to_owned(),
                    })
            }
        }
    };
}

define_generation!(AuthorityGeneration, "authority generation");
define_generation!(AdaptiveGeneration, "adaptive generation");

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub enum RevisionSemanticIntent {
    #[default]
    Create,
    Amend,
    Retract,
    Supersede,
}

impl RevisionSemanticIntent {
    #[must_use]
    pub const fn is_retraction(self) -> bool {
        matches!(self, Self::Retract)
    }
}

impl fmt::Display for RevisionSemanticIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Create => "create",
            Self::Amend => "amend",
            Self::Retract => "retract",
            Self::Supersede => "supersede",
        };
        formatter.write_str(value)
    }
}

impl FromStr for RevisionSemanticIntent {
    type Err = MemoriaError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "create" => Ok(Self::Create),
            "amend" => Ok(Self::Amend),
            "retract" => Ok(Self::Retract),
            "supersede" => Ok(Self::Supersede),
            value => Err(MemoriaError::Serialization(format!(
                "unknown revision semantic intent `{value}`"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthoritySnapshot {
    pub generation: AuthorityGeneration,
    pub captured_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveSnapshot {
    pub generation: AdaptiveGeneration,
    pub authority_generation: AuthorityGeneration,
    pub captured_at: Timestamp,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{AdaptiveGeneration, AuthorityGeneration, RevisionSemanticIntent};

    #[test]
    fn generations_start_at_zero_and_advance() {
        let authority = AuthorityGeneration::initial();
        assert_eq!(authority.value(), 0);
        assert_eq!(authority.next().value(), 1);
        assert_eq!(AdaptiveGeneration::new(7).value(), 7);
    }

    #[test]
    fn revision_semantic_intent_round_trips_as_stable_text() {
        let intent = RevisionSemanticIntent::Retract;
        assert_eq!(intent.to_string(), "retract");
        assert_eq!(RevisionSemanticIntent::from_str("retract").unwrap(), intent);
        assert!(intent.is_retraction());
    }
}
