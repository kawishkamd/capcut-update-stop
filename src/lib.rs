//! capcut-blocker: core library.
//!
//! The architecture that fixes the Python tool's structural problems:
//!
//! * `app` - pure, OS-agnostic domain logic (version selection, the change
//!   manifest, backup-path mapping). No UI, no Windows calls, so it is fully
//!   unit-testable on Linux. This is where correctness lives.
//! * `platform` - a tiny `SystemOps` trait for the handful of real OS actions,
//!   with a real Windows impl (`cfg(windows)`) and a `MockSystem` fake used by
//!   tests and non-Windows dev builds.
//!
//! The UI layer (added in milestone 2) only renders `app` state and sends
//! commands to a worker thread; it never touches OS state directly. That split
//! is what removes the Python version's thread-unsafe UI and guess-based restore.

pub mod app;
pub mod platform;
