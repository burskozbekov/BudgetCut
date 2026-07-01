//! Strongly-typed, sortable identifiers.
//!
//! Every entity uses a UUIDv7 id (§18): time-ordered so they sort naturally in
//! the op log and make good primary keys. Each entity gets its own newtype so
//! the type system prevents passing a `DetailId` where an `AccountId` is meant.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Declare a transparent newtype wrapper around [`Uuid`].
macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Mint a fresh, time-ordered id (UUIDv7).
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wrap an existing uuid (e.g. when rehydrating from storage).
            #[must_use]
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// The underlying uuid.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                core::fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<Uuid> for $name {
            fn from(u: Uuid) -> Self {
                Self(u)
            }
        }
    };
}

typed_id!(
    /// Identifies an organization — the top of the RBAC tree (server-side scope).
    OrgId
);
typed_id!(
    /// Identifies a project — a film/show/spot under an org (server-side scope).
    ProjectId
);
typed_id!(
    /// Identifies a [`crate::model::Budget`].
    BudgetId
);
typed_id!(
    /// Identifies a topsheet [`crate::model::Category`].
    CategoryId
);
typed_id!(
    /// Identifies an [`crate::model::Account`].
    AccountId
);
typed_id!(
    /// Identifies a [`crate::model::Detail`] cost line.
    DetailId
);
typed_id!(
    /// Identifies a [`crate::model::ProductionTotal`] divider.
    ProductionTotalId
);
typed_id!(
    /// Identifies a [`crate::model::Fringe`] tool.
    FringeId
);
typed_id!(
    /// Identifies a [`crate::model::Global`] variable.
    GlobalId
);
typed_id!(
    /// Identifies a [`crate::model::Unit`].
    UnitId
);
typed_id!(
    /// Identifies a [`crate::model::Group`].
    GroupId
);
typed_id!(
    /// Identifies a [`crate::model::Location`].
    LocationId
);
typed_id!(
    /// Identifies a [`crate::model::SetEntity`] (filming set).
    SetId
);
typed_id!(
    /// Identifies a [`crate::model::Currency`].
    CurrencyId
);
typed_id!(
    /// Identifies a user (the author of an op).
    UserId
);
typed_id!(
    /// Identifies a single mutation [`crate::ops::Op`]. Used for idempotent apply.
    OpId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_time_ordered_and_unique() {
        let a = DetailId::new();
        let b = DetailId::new();
        assert_ne!(a, b);
        // UUIDv7 is time-ordered: a minted earlier should sort before b.
        assert!(a < b, "uuidv7 ids should be monotonic within a process");
    }

    #[test]
    fn serde_roundtrip_is_transparent() {
        let id = AccountId::new();
        let json = serde_json::to_string(&id).unwrap();
        // Serialized as a bare string, not a wrapper object.
        assert!(json.starts_with('"'));
        let back: AccountId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
