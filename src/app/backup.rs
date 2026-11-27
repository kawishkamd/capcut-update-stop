//! Backup-path mapping.
//!
//! FIXES Python bug #2: backups were stored flat as `<filename>.bak` in one
//! folder, keyed only on the file's base name. Both `Apps/update.exe` and
//! `User Data/Download/update.exe` mapped to the same `update.exe.bak`, so only
//! the first original was ever saved and a restore could write the wrong binary
//! back to the wrong place.
//!
//! Here the backup path mirrors the file's path *relative to the CapCut root*,
//! so every distinct source file gets a distinct, collision-free backup.

use std::path::{Path, PathBuf};

/// Map a path relative to the CapCut install root to its backup location under
/// `backup_root`, mirroring the directory tree.
///
/// e.g. `Apps/update.exe`                 -> `<backup_root>/Apps/update.exe`
///      `User Data/Download/update.exe`   -> `<backup_root>/User Data/Download/update.exe`
pub fn backup_path_for(backup_root: &Path, rel_path: &Path) -> PathBuf {
    backup_root.join(rel_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_filename_different_dirs_do_not_collide() {
        let root = Path::new("/backup");
        let a = backup_path_for(root, Path::new("Apps/update.exe"));
        let b = backup_path_for(root, Path::new("User Data/Download/update.exe"));
        assert_ne!(
            a, b,
            "the Python bug: these must NOT be the same backup file"
        );
        assert_eq!(a, PathBuf::from("/backup/Apps/update.exe"));
        assert_eq!(b, PathBuf::from("/backup/User Data/Download/update.exe"));
    }
}
