//! Differential guard for the scope-atom budget (T0079): for tricky but
//! benign inputs, the atoms our splitters predict are exactly the atoms
//! syntect interns in its process-global repository. If a syntect upgrade
//! changes how scopes are split, this fails instead of silently reopening
//! the atom-flood hole. Its own test binary: it measures syntect's
//! process-global scope repository, which other tests in the same process
//! would change concurrently.

use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};

use ferris_pad::app::services::scope_atoms::ScopeAtomBudget;
use syntect::highlighting::ScopeSelectors;
use syntect::parsing::Scope;

/// The repository's next atom number. syntect numbers atoms in insertion
/// order and exposes no count, so intern a fresh probe atom and read its
/// number; the difference between two probes, minus one, is the number of
/// atoms interned in between.
fn probe() -> u16 {
    static PROBES: AtomicUsize = AtomicUsize::new(0);
    let n = PROBES.fetch_add(1, Ordering::Relaxed);
    // `Scope::new` interns into the global repository and the scope's first
    // atom number is that probe atom's position.
    Scope::new(&format!("zzprobe{n}")).unwrap().atom_at(0)
}

/// Atoms syntect interns while running `parse`.
fn interned_by(parse: impl FnOnce()) -> usize {
    let before = probe();
    parse();
    let after = probe();
    usize::from(after - before - 1)
}

#[test]
fn scope_atom_splitters_match_syntect() {
    // Every atom name is unique to its case, so all of them are new.
    let json_scopes = [
        "mjA1.mjA2",
        "  mjB1.mjB2  ",
        "mjC1.mjC2..",
        "",
        "   ",
        "mjD1 mjD2.mjD3",
        "mjE1,mjE2.mjE3 - mjE4",
        "mjF1|mjF2&(mjF3)",
        " mjG1 . mjG2 ",
    ];
    let mut failures = Vec::new();
    for s in json_scopes {
        let mut budget = ScopeAtomBudget::default();
        budget.admit_scopes([s]).unwrap();
        let predicted = budget.len();
        // How a JSON (serde) theme parses each scope string.
        let actual = interned_by(|| {
            let _: Scope = serde_json::from_value(serde_json::Value::String(s.into())).unwrap();
        });
        if predicted != actual {
            failures.push(format!(
                "JSON scope {s:?}: predicted {predicted}, syntect {actual}"
            ));
        }
    }

    let tm_selectors = [
        "mtA1.mtA2 mtA3.mtA4, mtB1 - mtC1 | mtD1",
        "mtE1-mtE2 -mtF1",
        "mtG1&mtG2 (mtH1)",
        "mtI1.. , mtJ1.",
        "  ,  | ",
        "\tmtK1\nmtL1",
    ];
    for s in tm_selectors {
        let mut budget = ScopeAtomBudget::default();
        budget.admit_tm_selectors([s]).unwrap();
        let predicted = budget.len();
        // How a .tmTheme's `scope` selector text is parsed.
        let actual = interned_by(|| {
            ScopeSelectors::from_str(s).unwrap();
        });
        if predicted != actual {
            failures.push(format!(
                "tmTheme selector {s:?}: predicted {predicted}, syntect {actual}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
