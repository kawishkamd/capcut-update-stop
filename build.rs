//! Build script: embed a `requireAdministrator` manifest into the Windows
//! executable (the Rust equivalent of PyInstaller's `--uac-admin`), so Windows
//! prompts for elevation at launch. This replaces the Python self-relaunch
//! dance entirely. No-op on every non-Windows target, so the Linux dev build is
//! unaffected.

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::manifest::ExecutionLevel;
        use embed_manifest::{embed_manifest, new_manifest};

        embed_manifest(
            new_manifest("CapCut.Blocker")
                .requested_execution_level(ExecutionLevel::RequireAdministrator),
        )
        .expect("failed to embed Windows manifest");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
