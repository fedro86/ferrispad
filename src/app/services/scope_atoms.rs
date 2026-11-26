//! Scope-atom budget for untrusted themes (T0079).
//!
//! syntect interns every atom of every scope it parses (the dot-separated
//! parts of `source.rust`) in a process-global repository
//! (`syntect::parsing::SCOPE_REPO`) that is never freed and holds at most
//! u16::MAX - 2 = 65 533 atoms. Atoms are interned *before* syntect rejects a
//! bad scope, so a theme file that fails to load can still fill the
//! repository; syntect's own syntaxes then panic when they load
//! (`SyntaxSet::load_defaults_newlines`). The built-in syntaxes and themes
//! use a few thousand atoms.
//!
//! So every theme is checked here before syntect sees any of it, counting
//! exactly the atoms syntect would intern (never fewer):
//! - a scope has at most 8 atoms (syntect's limit),
//! - a theme adds at most 2 048 atoms not seen before,
//! - all custom themes of the process add at most 8 192 atoms in total.
//!
//! The two splitters mirror syntect 5.3.0; `tests/scope_atom_mirror.rs`
//! checks them against syntect's real repository, so a syntect upgrade that
//! changes the splitting fails CI instead of reopening the hole.

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

/// Most atoms in one scope (syntect's own limit).
pub const MAX_ATOMS_PER_SCOPE: usize = 8;
/// Most new distinct atoms one theme may add.
pub const MAX_NEW_ATOMS_PER_THEME: usize = 2_048;
/// Most distinct atoms all custom themes of the process may add.
pub const MAX_CUSTOM_ATOMS_TOTAL: usize = 8_192;

/// The atoms syntect interns for ONE scope string `s`.
///
/// Mirrors syntect 5.3.0 `Scope::new` (`src/parsing/scope.rs:237-239`:
/// `build(s.trim())`) and `ScopeRepository::build` (`scope.rs:181-188`): an
/// empty string has no atoms, otherwise `trim_end_matches('.')` then
/// `split('.')`. Nothing else splits: spaces, commas and the like stay
/// inside an atom. This is how every scope string of a JSON (serde) theme
/// is parsed (`impl Deserialize for Scope`, `scope.rs:380-384`).
pub fn scope_atoms(s: &str) -> Vec<&str> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    s.trim_end_matches('.').split('.').collect()
}

/// The scope strings syntect parses out of one `.tmTheme` selector text.
///
/// Mirrors syntect 5.3.0: `ScopeSelectors::from_str` splits on `,` and `|`
/// (`src/highlighting/selector.rs:116`), `ScopeSelector::from_str` on `" -"`
/// (`selector.rs:72`), `ScopeStack::from_str` on whitespace
/// (`src/parsing/scope.rs:576`); each piece is then a scope for
/// [`scope_atoms`].
pub fn tm_selector_scopes(selector: &str) -> Vec<&str> {
    selector
        .split([',', '|'])
        .flat_map(|sel| sel.split(" -"))
        .flat_map(str::split_whitespace)
        .collect()
}

/// The scope atoms admitted so far, with the per-theme and total limits.
#[derive(Debug)]
pub struct ScopeAtomBudget {
    seen: HashSet<String>,
    per_theme: usize,
    total: usize,
}

impl Default for ScopeAtomBudget {
    fn default() -> Self {
        Self::new(MAX_NEW_ATOMS_PER_THEME, MAX_CUSTOM_ATOMS_TOTAL)
    }
}

impl ScopeAtomBudget {
    pub fn new(per_theme: usize, total: usize) -> Self {
        Self {
            seen: HashSet::new(),
            per_theme,
            total,
        }
    }

    /// Check the scope strings of one theme (each string is one scope, as in
    /// a JSON theme). On success their atoms count as seen; on failure
    /// nothing is recorded.
    pub fn admit_scopes<'a>(
        &mut self,
        scopes: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        let mut new: HashSet<&str> = HashSet::new();
        for scope in scopes {
            let atoms = scope_atoms(scope);
            if atoms.len() > MAX_ATOMS_PER_SCOPE {
                return Err(format!("a scope has more than {MAX_ATOMS_PER_SCOPE} parts"));
            }
            new.extend(atoms.into_iter().filter(|a| !self.seen.contains(*a)));
            if new.len() > self.per_theme {
                return Err(format!("more than {} distinct scope names", self.per_theme));
            }
        }
        if self.seen.len() + new.len() > self.total {
            return Err(format!(
                "custom themes together exceed {} distinct scope names",
                self.total
            ));
        }
        self.seen.extend(new.into_iter().map(str::to_string));
        Ok(())
    }

    /// [`Self::admit_scopes`] for `.tmTheme` selector texts.
    pub fn admit_tm_selectors<'a>(
        &mut self,
        selectors: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), String> {
        self.admit_scopes(selectors.into_iter().flat_map(tm_selector_scopes))
    }

    /// How many atoms have been admitted.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// The budget shared by every custom theme loaded or imported in this
/// process, like the syntect repository it protects.
static PROCESS_ATOM_BUDGET: Mutex<Option<ScopeAtomBudget>> = Mutex::new(None);

/// Run `f` with the process-wide atom budget.
pub fn with_process_atom_budget<T>(f: impl FnOnce(&mut ScopeAtomBudget) -> T) -> T {
    let mut guard = PROCESS_ATOM_BUDGET
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(ScopeAtomBudget::default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_atoms_mirror_syntect_build() {
        assert_eq!(scope_atoms("source.rust"), vec!["source", "rust"]);
        assert_eq!(scope_atoms("  a.b  "), vec!["a", "b"]);
        assert_eq!(scope_atoms("a.b.."), vec!["a", "b"]);
        assert!(scope_atoms("").is_empty());
        assert!(scope_atoms("   ").is_empty());
        // Spaces and selector punctuation are part of an atom here.
        assert_eq!(scope_atoms("a b.c, d"), vec!["a b", "c, d"]);
        assert_eq!(scope_atoms("...."), vec![""]);
    }

    #[test]
    fn tm_selector_scopes_mirror_syntect_selectors() {
        assert_eq!(
            tm_selector_scopes("a.b c.d, e - f | g"),
            vec!["a.b", "c.d", "e", "f", "g"]
        );
        // `-` only splits after a space; `&` and parentheses stay in atoms.
        assert_eq!(tm_selector_scopes("a-b -c"), vec!["a-b", "c"]);
        assert_eq!(tm_selector_scopes("x&y (z)"), vec!["x&y", "(z)"]);
        assert!(tm_selector_scopes("  ,  | ").is_empty());
    }

    #[test]
    fn scope_atom_budget_verdicts() {
        let mut budget = ScopeAtomBudget::new(5, 7);
        // JSON scopes: "a b" is ONE atom. Atoms: "a b", "c", "d".
        assert!(budget.admit_scopes(["a b.c", "d"]).is_ok());
        assert_eq!(budget.len(), 3);
        // Already-seen atoms are free.
        assert!(budget.admit_scopes(["c.d.a b"]).is_ok());
        // Per theme: 6 new atoms > 5.
        assert!(budget.admit_scopes(["p.q.r.s.t.u"]).is_err());
        // A rejection records nothing: 4 new atoms still fit (3 + 4 = 7).
        assert!(budget.admit_scopes(["w.x.y.z"]).is_ok());
        // Total: one more new atom exceeds 7.
        assert!(budget.admit_scopes(["v"]).is_err());
        // Too many parts in one scope, even with known atoms.
        let mut fresh = ScopeAtomBudget::default();
        assert!(fresh.admit_scopes(["a.a.a.a.a.a.a.a.a"]).is_err());
        assert!(
            fresh.admit_scopes(["a.a.a.a.a.a.a.a."]).is_ok(),
            "trailing dot ignored"
        );
        // .tmTheme selectors are split like syntect: 3 scopes of 3 atoms.
        let mut tm = ScopeAtomBudget::default();
        assert!(
            tm.admit_tm_selectors(["s1.s2.s3 s4.s5.s6, s7.s8.s9"])
                .is_ok()
        );
        assert_eq!(tm.len(), 9);
        // The flood: 66 000 distinct atoms in one scope string.
        let flood: Vec<String> = (0..66_000).map(|i| format!("f{i}")).collect();
        assert!(fresh.admit_scopes([flood.join(".").as_str()]).is_err());
    }
}
