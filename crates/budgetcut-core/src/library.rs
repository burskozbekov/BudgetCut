//! Reusable setup library (MMB "cloud-synched libraries of frequently used
//! setup items"). A library is a portable bundle of fringe presets and global
//! variables that can be lifted out of one budget and applied to another, so a
//! production company keeps one canonical set (SGK/stopaj/KDV, shoot-day
//! globals, …). Pure; the sync/storage layer is plumbing on top.

use crate::ids::{FringeId, GlobalId};
use crate::{Budget, Fringe, Global};
use serde::{Deserialize, Serialize};

/// A portable bundle of setup items, keyed by stable `code` for de-duplication.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupLibrary {
    pub name: String,
    pub fringes: Vec<Fringe>,
    pub globals: Vec<Global>,
}

impl SetupLibrary {
    /// Lift every fringe preset and global out of a budget into a library.
    #[must_use]
    pub fn extract(name: impl Into<String>, budget: &Budget) -> Self {
        let mut fringes: Vec<Fringe> = budget.fringes.values().cloned().collect();
        fringes.sort_by(|a, b| a.code.cmp(&b.code));
        let mut globals: Vec<Global> = budget.globals.values().cloned().collect();
        globals.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            name: name.into(),
            fringes,
            globals,
        }
    }

    /// Apply the library into `budget`, adding items whose `code`/`name` aren't
    /// already present. Existing items are left untouched (no clobber). Returns
    /// how many fringes and globals were newly added.
    pub fn apply_into(&self, budget: &mut Budget) -> (usize, usize) {
        let have_fringe: std::collections::BTreeSet<String> =
            budget.fringes.values().map(|f| f.code.clone()).collect();
        let have_global: std::collections::BTreeSet<String> =
            budget.globals.values().map(|g| g.name.clone()).collect();

        let mut added_f = 0;
        for f in &self.fringes {
            if !have_fringe.contains(&f.code) {
                let mut f = f.clone();
                f.id = FringeId::new();
                budget.fringes.insert(f.id, f);
                added_f += 1;
            }
        }
        let mut added_g = 0;
        for g in &self.globals {
            if !have_global.contains(&g.name) {
                let mut g = g.clone();
                g.id = GlobalId::new();
                budget.globals.insert(g.id, g);
                added_g += 1;
            }
        }
        (added_f, added_g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::GlobalId;
    use crate::{templates, Formula, Global, Localized};
    use rust_decimal::Decimal;

    #[test]
    fn extract_then_apply_clones_setup_without_duplicating() {
        // Source: the seeded Turkish dizi template (has fringes) + a global.
        let mut src = templates::turkish_dizi_template("Kaynak");
        let g = Global {
            id: GlobalId::new(),
            name: "CEKIM_GUN".into(),
            description: Localized::tr("Çekim günü"),
            value: Formula::Const(Decimal::from(45)),
            in_budget_total: true,
        };
        src.globals.insert(g.id, g);
        let lib = SetupLibrary::extract("TR Standart", &src);
        assert!(!lib.fringes.is_empty());
        assert!(!lib.globals.is_empty());

        // Fresh empty budget gets the whole library.
        let mut blank = Budget::new("Yeni Proje", templates::try_currency());
        let (f1, g1) = lib.apply_into(&mut blank);
        assert_eq!(f1, lib.fringes.len());
        assert_eq!(g1, lib.globals.len());

        // Applying again is idempotent by code/name — nothing duplicated.
        let (f2, g2) = lib.apply_into(&mut blank);
        assert_eq!((f2, g2), (0, 0));
        assert_eq!(blank.fringes.len(), lib.fringes.len());
    }
}
