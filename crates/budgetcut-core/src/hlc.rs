//! Hybrid Logical Clock (§8).
//!
//! Each op carries an [`Hlc`] timestamp. HLCs give us a **total order** across
//! all nodes that stays close to wall-clock time, which is exactly what
//! per-field Last-Write-Wins needs: the op with the higher HLC wins a contested
//! field, deterministically, regardless of arrival order.
//!
//! The clock is pure: it never reads the system clock itself. The shell (Tauri
//! client / Axum server) passes in the current wall time in milliseconds, so
//! the core stays I/O-free and fully testable (§4).

use serde::{Deserialize, Serialize};

use crate::ids::UserId;

/// A node in the sync topology (a client device or the server). Ties in
/// `(wall, counter)` are broken by node id, giving HLCs a strict total order.
pub type NodeId = UserId;

/// A hybrid logical clock timestamp.
///
/// Ordering is lexicographic over `(wall_ms, counter, node)`. Because `node` is
/// unique per participant and `counter` is monotonic per node, two distinct
/// events from a *continuously running* (or properly [`HlcClock::seeded`])
/// clock can never compare equal — the order is total. A clock restarted via
/// [`HlcClock::new`] resets its counter and could re-issue a past HLC, so
/// always rehydrate on restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hlc {
    /// Physical time component, milliseconds since the Unix epoch.
    pub wall_ms: u64,
    /// Logical counter, bumped when physical time doesn't advance.
    pub counter: u32,
    /// The node that minted this timestamp (tiebreaker).
    pub node: NodeId,
}

impl Hlc {
    #[must_use]
    pub fn new(wall_ms: u64, counter: u32, node: NodeId) -> Self {
        Self {
            wall_ms,
            counter,
            node,
        }
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.wall_ms
            .cmp(&other.wall_ms)
            .then(self.counter.cmp(&other.counter))
            .then(self.node.cmp(&other.node))
    }
}

/// A per-node clock that generates monotonically increasing [`Hlc`]s.
///
/// Implements the standard HLC update rules (Kulkarni et al.): `tick` is called
/// when producing a local event; `observe` is called on receiving a remote HLC
/// to keep the local clock ahead of anything it has seen.
#[derive(Debug, Clone)]
pub struct HlcClock {
    node: NodeId,
    last: Hlc,
}

impl HlcClock {
    /// Create a clock for `node`, seeded behind any real timestamp.
    ///
    /// **On restart, do not use `new` alone.** A fresh clock restarts at
    /// `counter = 0`, so it could re-mint an HLC equal to one issued before the
    /// crash/reload (breaking the strict-total-order guarantee). Rehydrate with
    /// [`HlcClock::seeded`] from the maximum HLC observed in the persisted op
    /// log before accepting new local ops.
    #[must_use]
    pub fn new(node: NodeId) -> Self {
        Self {
            node,
            last: Hlc::new(0, 0, node),
        }
    }

    /// Create a clock for `node` seeded from a previously-observed HLC (e.g. the
    /// max HLC in the loaded op log), so newly minted timestamps strictly exceed
    /// everything seen before a restart.
    #[must_use]
    pub fn seeded(node: NodeId, last: Hlc) -> Self {
        Self { node, last }
    }

    /// The most recent timestamp this clock emitted or observed.
    #[must_use]
    pub fn last(&self) -> Hlc {
        self.last
    }

    /// Produce a new local timestamp given the current physical time (ms).
    ///
    /// If the wall clock moved forward, reset the counter; otherwise bump the
    /// counter so the new HLC strictly exceeds the previous one.
    pub fn tick(&mut self, physical_ms: u64) -> Hlc {
        let wall = physical_ms.max(self.last.wall_ms);
        let counter = if wall == self.last.wall_ms {
            self.last.counter + 1
        } else {
            0
        };
        self.last = Hlc::new(wall, counter, self.node);
        self.last
    }

    /// Update the clock having observed a remote timestamp, then return a fresh
    /// local timestamp that exceeds both local history and the remote event.
    pub fn observe(&mut self, remote: Hlc, physical_ms: u64) -> Hlc {
        let wall = physical_ms.max(self.last.wall_ms).max(remote.wall_ms);
        let counter = if wall == self.last.wall_ms && wall == remote.wall_ms {
            self.last.counter.max(remote.counter) + 1
        } else if wall == self.last.wall_ms {
            self.last.counter + 1
        } else if wall == remote.wall_ms {
            remote.counter + 1
        } else {
            0
        };
        self.last = Hlc::new(wall, counter, self.node);
        self.last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> NodeId {
        UserId::new()
    }

    #[test]
    fn ticks_are_strictly_increasing() {
        let mut c = HlcClock::new(node());
        let a = c.tick(1000);
        let b = c.tick(1000); // same wall time -> counter bumps
        let d = c.tick(2000); // wall advances -> counter resets
        assert!(a < b);
        assert!(b < d);
        assert_eq!(b.counter, 1);
        assert_eq!(d.counter, 0);
    }

    #[test]
    fn clock_stays_monotonic_even_if_wall_goes_backwards() {
        let mut c = HlcClock::new(node());
        let a = c.tick(5000);
        let b = c.tick(1000); // wall jumped backwards
        assert!(b > a, "hlc must never regress");
    }

    #[test]
    fn observe_jumps_ahead_of_remote() {
        let n1 = node();
        let n2 = node();
        let remote = Hlc::new(9000, 3, n2);
        let mut c = HlcClock::new(n1);
        let local = c.observe(remote, 1000);
        assert!(local > remote);
    }

    #[test]
    fn total_order_breaks_ties_by_node() {
        // Two nodes mint at identical wall/counter; node id breaks the tie.
        let n1 = UserId::from_uuid(uuid::Uuid::from_u128(1));
        let n2 = UserId::from_uuid(uuid::Uuid::from_u128(2));
        let a = Hlc::new(1000, 0, n1);
        let b = Hlc::new(1000, 0, n2);
        assert!(a != b);
        assert!(a < b);
    }
}
