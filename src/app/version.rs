//! Version-folder selection.
//!
//! FIXES Python bug #1 (the destructive one): `clean_old_versions` sorted the
//! `Apps/<version>` folders descending and `shutil.rmtree`'d everything except
//! the HIGHEST-numbered one - so if you pinned 1.5.0, it got silently deleted
//! while 8.2.0 was kept. The opposite of the tool's purpose, and irreversible.
//!
//! Here we (a) let the caller pin the version to keep, and (b) NEVER delete the
//! others - they are returned as untouched "other" folders. Any cleanup you opt
//! into is a *move-aside*, recorded in the manifest and reversible.

use std::path::PathBuf;

/// A CapCut version directory discovered under `Apps/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledVersion {
    /// Folder name, e.g. "1.5.0".
    pub name: String,
    /// Full path to the version folder.
    pub path: PathBuf,
    /// Parsed numeric components for ordering, e.g. [1, 5, 0].
    pub parts: Vec<u32>,
}

impl InstalledVersion {
    pub fn parse(name: &str, path: PathBuf) -> Self {
        let parts = name
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect();
        InstalledVersion {
            name: name.to_string(),
            path,
            parts,
        }
    }
}

/// The decision about which installed version to protect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepDecision {
    /// The version we protect and lock.
    pub keep: InstalledVersion,
    /// Every other installed version. These are LEFT ALONE unless you
    /// explicitly opt into a (reversible, manifest-recorded) move-aside.
    pub others: Vec<InstalledVersion>,
}

/// Choose which version to keep.
///
/// * `pin = Some(name)` and it exists  -> keep exactly that version.
/// * otherwise                         -> keep the highest version.
///
/// In every case the others are returned untouched; this function never decides
/// to delete anything.
pub fn choose_keep(mut versions: Vec<InstalledVersion>, pin: Option<&str>) -> Option<KeepDecision> {
    if versions.is_empty() {
        return None;
    }
    // Highest version first (stable, numeric component comparison).
    versions.sort_by(|a, b| b.parts.cmp(&a.parts).then_with(|| a.name.cmp(&b.name)));

    let keep_idx = pin
        .and_then(|p| versions.iter().position(|v| v.name == p))
        .unwrap_or(0);

    let keep = versions.remove(keep_idx);
    Some(KeepDecision {
        keep,
        others: versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> InstalledVersion {
        InstalledVersion::parse(name, PathBuf::from(format!("/Apps/{name}")))
    }

    #[test]
    fn pinning_old_version_keeps_it_not_the_highest() {
        // The exact Python failure: 1.5.0 should be kept, but 8.2.0 is present.
        let found = vec![v("8.2.0"), v("2.0.0"), v("1.5.0")];
        let d = choose_keep(found, Some("1.5.0")).unwrap();
        assert_eq!(d.keep.name, "1.5.0");
        // Nothing is marked for deletion; the others are just returned.
        assert_eq!(d.others.len(), 2);
        assert!(d.others.iter().any(|x| x.name == "8.2.0"));
    }

    #[test]
    fn without_pin_keeps_highest() {
        let found = vec![v("1.5.0"), v("8.2.0"), v("2.0.0")];
        let d = choose_keep(found, None).unwrap();
        assert_eq!(d.keep.name, "8.2.0");
        assert_eq!(d.others.len(), 2);
    }

    #[test]
    fn numeric_order_not_lexical() {
        // "10.0.0" must beat "9.0.0"; string sort would get this wrong.
        let found = vec![v("9.0.0"), v("10.0.0")];
        let d = choose_keep(found, None).unwrap();
        assert_eq!(d.keep.name, "10.0.0");
    }

    #[test]
    fn empty_is_none() {
        assert!(choose_keep(vec![], None).is_none());
    }

    #[test]
    fn unknown_pin_falls_back_to_highest() {
        let found = vec![v("1.5.0"), v("2.0.0")];
        let d = choose_keep(found, Some("99.0.0")).unwrap();
        assert_eq!(d.keep.name, "2.0.0");
    }
}
