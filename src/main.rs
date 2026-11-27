// Hide the console window on Windows release builds (the Rust equivalent of
// PyInstaller's `--noconsole`). Dev builds keep the console for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Thin entry point: all logic is in the `capcut_blocker` library; the UI is in
//! `ui`. On Windows the embedded manifest (see `build.rs`) makes Windows elevate
//! at launch, so there is no self-relaunch dance. On other platforms this runs
//! as a harmless dev build against the in-memory `MockSystem`.

mod ui;

fn main() -> eframe::Result<()> {
    ui::run()
}
