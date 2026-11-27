//! Real Windows implementation of `SystemOps`.
//!
//! Compiled only on Windows (`cfg(windows)`), so it never breaks the Linux dev
//! build. Read-only toggling goes through `std::fs` (which maps to the Windows
//! read-only attribute); process detection uses the ToolHelp snapshot API -
//! replacing the Python `tasklist` subprocess, with no console-window flash.
//!
//! Type-checked from Linux with `cargo check --target x86_64-pc-windows-gnu`
//! (checking needs no linker), and compiled for real by CI on `windows-latest`.

use super::SystemOps;
use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

pub struct WindowsSystem;

impl SystemOps for WindowsSystem {
    fn process_running(&self, image_name: &str) -> bool {
        // SAFETY: standard ToolHelp walk. We check the snapshot handle, zero the
        // entry and set `dwSize` as the API requires, and always close the
        // handle before returning.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
                return false;
            }

            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

            let mut found = false;
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    if name.eq_ignore_ascii_case(image_name) {
                        found = true;
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }

            CloseHandle(snapshot);
            found
        }
    }

    fn set_readonly(&self, path: &Path, readonly: bool) -> io::Result<()> {
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_readonly(readonly);
        std::fs::set_permissions(path, perms)
    }

    fn is_readonly(&self, path: &Path) -> io::Result<bool> {
        Ok(std::fs::metadata(path)?.permissions().readonly())
    }

    fn set_deny_write(&self, path: &Path, deny: bool) -> io::Result<()> {
        // Shell out to the built-in `icacls` rather than hand-rolled security-
        // descriptor FFI: it is stable, well-documented, and its removal is
        // precise (`/remove:d` deletes ONLY deny ACEs for the named SID, leaving
        // any allow ACEs untouched - so restore can't drift the permissions).
        // The recovery command we document for users is exactly this call.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000; // no console flash
        const EVERYONE_SID: &str = "*S-1-1-0";

        let mut cmd = std::process::Command::new("icacls");
        cmd.arg(path);
        if deny {
            // Deny Write + Delete. `(W,D)` excludes WRITE_DAC, so an admin (and
            // thus our restore) can still reset the ACL afterwards.
            cmd.arg("/deny").arg(format!("{EVERYONE_SID}:(W,D)"));
        } else {
            cmd.arg("/remove:d").arg(EVERYONE_SID);
        }

        let out = cmd.creation_flags(CREATE_NO_WINDOW).output()?;
        if out.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "icacls failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
    }
}
