//! The change manifest.
//!
//! FIXES Python bug #3 (guess-based restore): the Python `is_file_blocked`
//! heuristics sniffed state - "exe smaller than 1 MB == blocked", "configure.ini
//! contains last_version=1.0.0.0", "ProductInfo.xml is 0 bytes", "png <= 1 KB".
//! Any of those can be true for innocent reasons, so restore could skip a real
//! backup or delete a legitimate file.
//!
//! Instead, blocking writes a manifest recording EXACTLY what it did to each
//! file (action, original hash, backup location). Restore is driven entirely by
//! this record - no guessing. If there is no manifest, there is nothing to undo.

use serde::{Deserialize, Serialize};

/// What the blocker did to one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Existing file made read-only; content untouched. Restore: clear read-only.
    LockedReadonly,
    /// File content was replaced (configure.ini pin, transparent watermark).
    /// Restore: copy the backup back, then clear read-only.
    ReplacedContent,
    /// We created a 0-byte placeholder where no file existed, to occupy the
    /// updater's path. Restore: delete the placeholder.
    CreatedPlaceholder,
    /// Original file was moved aside (e.g. an unwanted version folder).
    /// Restore: move it back from `backup_rel`.
    MovedAside,
    /// A Deny-Write ACL entry (Everyone) was added on top of the lock - the
    /// opt-in "stronger lock" (`BlockConfig::harden_acl`). Unlike read-only, a
    /// Deny cannot be bypassed by a normal write, only removed by an admin.
    /// Restore: remove the Deny entry. Recorded last, so reverse-order restore
    /// lifts every Deny before any content is touched.
    AclDenyWrite,
}

/// One recorded change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEntry {
    /// Path relative to the CapCut install root.
    pub rel_path: String,
    pub action: Action,
    /// SHA-256 of the original file, when one existed. Lets restore verify it is
    /// putting back what it took, and detect tampering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_sha256: Option<String>,
    /// Backup location relative to the backup root, when a backup was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_rel: Option<String>,
}

/// The full record written on "Block", read on "Restore".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// On-disk format version, so future tools can migrate old manifests.
    pub format: u32,
    pub created_utc: String,
    /// The CapCut version that was pinned.
    pub capcut_version: String,
    pub entries: Vec<ChangeEntry>,
}

impl Manifest {
    pub const FORMAT: u32 = 1;

    pub fn new(capcut_version: impl Into<String>, created_utc: impl Into<String>) -> Self {
        Manifest {
            format: Self::FORMAT,
            created_utc: created_utc.into(),
            capcut_version: capcut_version.into(),
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: ChangeEntry) {
        self.entries.push(entry);
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let mut m = Manifest::new("1.5.0", "2026-11-26T00:00:00Z");
        m.push(ChangeEntry {
            rel_path: "Apps/configure.ini".into(),
            action: Action::ReplacedContent,
            original_sha256: Some("abc123".into()),
            backup_rel: Some("Apps/configure.ini".into()),
        });
        m.push(ChangeEntry {
            rel_path: "Apps/update.exe".into(),
            action: Action::CreatedPlaceholder,
            original_sha256: None,
            backup_rel: None,
        });

        let json = m.to_json().unwrap();
        let back = Manifest::from_json(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn omits_empty_optionals_in_json() {
        let mut m = Manifest::new("1.5.0", "2026-11-26T00:00:00Z");
        m.push(ChangeEntry {
            rel_path: "Apps/update.exe".into(),
            action: Action::CreatedPlaceholder,
            original_sha256: None,
            backup_rel: None,
        });
        let json = m.to_json().unwrap();
        assert!(!json.contains("original_sha256"));
        assert!(!json.contains("backup_rel"));
    }
}
