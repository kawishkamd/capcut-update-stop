//! The OS boundary.
//!
//! Every real operating-system action the blocker needs is behind this one
//! small trait. The core logic depends only on the trait, so it is testable on
//! Linux with `MockSystem`; the real behaviour lives in `windows.rs` and is the
//! only code that is `cfg(windows)`.
//!
//! This is also where Python bug #4 gets addressed honestly: the Python tool's
//! only "lock" was the read-only attribute, which any process can clear. The
//! trait leaves room for a stronger `deny_write_acl` (a Windows ACE denying
//! WRITE to the updater) added in the hardening milestone, while `set_readonly`
//! remains the baseline.

use std::io;
use std::path::Path;

/// The OS actions the blocker performs. Kept intentionally tiny.
pub trait SystemOps {
    /// Is a process with this image name (e.g. "CapCut.exe") running?
    fn process_running(&self, image_name: &str) -> bool;

    /// Set or clear the read-only attribute on a file.
    fn set_readonly(&self, path: &Path, readonly: bool) -> io::Result<()>;

    /// Is the file currently read-only?
    fn is_readonly(&self, path: &Path) -> io::Result<bool>;

    /// Add (`deny = true`) or remove (`deny = false`) a Deny-Write access-control
    /// entry for Everyone on `path` - the opt-in "stronger lock". A Deny cannot
    /// be bypassed by a normal write (Deny wins over Allow in Windows), only
    /// removed by an administrator, which the restore flow is. The Windows impl
    /// shells out to `icacls`; `MockSystem` just records the state.
    fn set_deny_write(&self, path: &Path, deny: bool) -> io::Result<()>;
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::WindowsSystem;

/// In-memory fake used by unit tests and non-Windows dev builds. Records the
/// read-only state it is told to set, so core logic can be exercised with no
/// real filesystem or OS.
#[derive(Default)]
pub struct MockSystem {
    running: std::cell::RefCell<bool>,
    readonly: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, bool>>,
    deny_write: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, bool>>,
}

impl MockSystem {
    pub fn set_running(&self, running: bool) {
        *self.running.borrow_mut() = running;
    }

    /// Test helper: is a Deny-Write entry currently recorded for `path`?
    pub fn is_deny_write(&self, path: &Path) -> bool {
        *self.deny_write.borrow().get(path).unwrap_or(&false)
    }
}

impl SystemOps for MockSystem {
    fn process_running(&self, _image_name: &str) -> bool {
        *self.running.borrow()
    }

    fn set_readonly(&self, path: &Path, readonly: bool) -> io::Result<()> {
        self.readonly
            .borrow_mut()
            .insert(path.to_path_buf(), readonly);
        Ok(())
    }

    fn is_readonly(&self, path: &Path) -> io::Result<bool> {
        Ok(*self.readonly.borrow().get(path).unwrap_or(&false))
    }

    fn set_deny_write(&self, path: &Path, deny: bool) -> io::Result<()> {
        self.deny_write
            .borrow_mut()
            .insert(path.to_path_buf(), deny);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn mock_tracks_readonly_state() {
        let sys = MockSystem::default();
        let p = Path::new("Apps/configure.ini");
        assert!(!sys.is_readonly(p).unwrap());
        sys.set_readonly(p, true).unwrap();
        assert!(sys.is_readonly(p).unwrap());
        sys.set_readonly(p, false).unwrap();
        assert!(!sys.is_readonly(p).unwrap());
    }

    #[test]
    fn mock_tracks_running_flag() {
        let sys = MockSystem::default();
        assert!(!sys.process_running("CapCut.exe"));
        sys.set_running(true);
        assert!(sys.process_running("CapCut.exe"));
    }

    #[test]
    fn mock_tracks_deny_write_state() {
        let sys = MockSystem::default();
        let p = Path::new("Apps/update.exe");
        assert!(!sys.is_deny_write(p));
        sys.set_deny_write(p, true).unwrap();
        assert!(sys.is_deny_write(p));
        sys.set_deny_write(p, false).unwrap();
        assert!(!sys.is_deny_write(p));
    }
}
