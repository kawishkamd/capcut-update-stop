//! The block/restore engine.
//!
//! Ties the pure pieces together and performs the actual filesystem work, but
//! routes the two genuinely OS-specific actions (read-only toggling, "is CapCut
//! running") through `SystemOps`. Because plain file I/O is cross-platform, the
//! whole engine is exercised on Linux with a temp dir + `MockSystem` - see the
//! tests at the bottom, which reproduce the Python backup-collision scenario and
//! prove a round-trip restore.
//!
//! Design choices that fix the Python tool:
//!   * old version folders are NEVER deleted here (fix #1);
//!   * backups mirror the relative path, so the two `update.exe`s can't collide
//!     (fix #2);
//!   * every change is written to a manifest, and restore is driven solely by
//!     that record - no state-sniffing heuristics (fix #3).

use crate::app::backup::backup_path_for;
use crate::app::manifest::{Action, ChangeEntry, Manifest};
use crate::app::version::{choose_keep, InstalledVersion};
use crate::platform::SystemOps;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// File name of the change record, stored in the backup root.
pub const MANIFEST_NAME: &str = "capcut-blocker.manifest.json";
/// Process image we refuse to run alongside.
const PROCESS_IMAGE: &str = "CapCut.exe";

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("CapCut is running - close it before blocking or restoring")]
    CapCutRunning,
    #[error("CapCut install not found at {0}")]
    NotInstalled(PathBuf),
    #[error("nothing to restore (no manifest at the expected location)")]
    NoManifest,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("manifest serialization: {0}")]
    Manifest(#[from] serde_json::Error),
}

/// Knobs for a block run.
pub struct BlockConfig<'a> {
    /// Which installed version to protect. `None` => highest installed.
    pub pin_version: Option<&'a str>,
    /// Value written into `configure.ini`'s `last_version=` to freeze it.
    pub pinned_last_version: &'a str,
    /// Opt-in "stronger lock": after the read-only lock, also apply a Deny-Write
    /// ACL to the frozen files (see `SystemOps::set_deny_write`). Off by default;
    /// validate a full Block -> Restore on real Windows before enabling, since
    /// the ACL path can only be compile-checked off-Windows.
    pub harden_acl: bool,
    /// Replace CapCut's export watermark (`CClogo-w.png`) with a transparent
    /// image in every installed version. Only touches files that already exist.
    pub remove_watermark: bool,
}

impl Default for BlockConfig<'_> {
    fn default() -> Self {
        BlockConfig {
            pin_version: None,
            pinned_last_version: "1.0.0.0",
            harden_acl: false,
            remove_watermark: false,
        }
    }
}

/// Is a CapCut install present under `root`?
pub fn is_capcut_installed(root: &Path) -> bool {
    let apps = root.join("Apps");
    if !apps.is_dir() {
        return false;
    }
    if apps.join("CapCut.exe").exists() {
        return true;
    }
    if let Ok(rd) = fs::read_dir(&apps) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir()
                && p.file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|n| n.chars().next())
                    .is_some_and(|c| c.is_ascii_digit())
                && p.join("CapCut.exe").exists()
            {
                return true;
            }
        }
    }
    false
}

/// Has a block run already been applied (a manifest exists)?
pub fn is_blocked(backup_root: &Path) -> bool {
    backup_root.join(MANIFEST_NAME).exists()
}

/// Installed version-folder names under `Apps/`, highest first. For the UI's
/// "protect this version" selector - the input to the non-destructive fix #1.
pub fn installed_versions(root: &Path) -> Vec<String> {
    let mut v = discover_versions(&root.join("Apps"));
    v.sort_by(|a, b| b.parts.cmp(&a.parts).then_with(|| a.name.cmp(&b.name)));
    v.into_iter().map(|x| x.name).collect()
}

/// Apply the block. Returns the manifest that was written.
pub fn block<S: SystemOps>(
    root: &Path,
    backup_root: &Path,
    sys: &S,
    cfg: &BlockConfig,
) -> Result<Manifest, EngineError> {
    if sys.process_running(PROCESS_IMAGE) {
        return Err(EngineError::CapCutRunning);
    }
    if !root.exists() {
        return Err(EngineError::NotInstalled(root.to_path_buf()));
    }

    let apps = root.join("Apps");
    let download = root.join("User Data").join("Download");

    // Decide which version to keep - purely to record it; we never delete the
    // others (that is the whole point of fix #1).
    let versions = discover_versions(&apps);
    let kept_name = choose_keep(versions, cfg.pin_version)
        .map(|d| d.keep.name)
        .unwrap_or_default();

    let mut manifest = Manifest::new(kept_name, now_iso8601());

    // 1. Freeze + lock configure.ini.
    pin_configure_ini(
        &apps.join("configure.ini"),
        root,
        backup_root,
        cfg.pinned_last_version,
        sys,
        &mut manifest,
    )?;

    // 2. Lock ProductInfo.xml read-only WITHOUT touching its contents.
    let product_info = apps.join("ProductInfo.xml");
    if product_info.exists() {
        lock_readonly(&product_info, root, sys, &mut manifest)?;
    }

    // 3. Neutralize both updater binaries.
    block_updater(
        &apps.join("update.exe"),
        root,
        backup_root,
        sys,
        &mut manifest,
    )?;
    block_updater(
        &download.join("update.exe"),
        root,
        backup_root,
        sys,
        &mut manifest,
    )?;

    // 4. Optional: swap the export watermark for a transparent image.
    if cfg.remove_watermark {
        replace_watermarks(root, backup_root, sys, &mut manifest)?;
    }

    // 5. Optional stronger lock: Deny-Write ACL on top of the read-only locks.
    if cfg.harden_acl {
        harden_with_acl(root, sys, &mut manifest)?;
    }

    // Persist the record.
    fs::create_dir_all(backup_root)?;
    fs::write(backup_root.join(MANIFEST_NAME), manifest.to_json()?)?;

    Ok(manifest)
}

/// Transparent stand-in for CapCut's export watermark, embedded at build time
/// (no runtime extraction). Same asset the Python version shipped.
const TRANSPARENT_WATERMARK: &[u8] = include_bytes!("../../assets/CClogo-w.png");
const WATERMARK_NAME: &str = "CClogo-w.png";

/// Replace the export watermark in every installed version with the transparent
/// image, backing up each original first. Only touches files that already exist -
/// it never creates a watermark where CapCut didn't put one. Recorded as normal
/// `ReplacedContent`, so Restore puts the originals back with no special-casing.
fn replace_watermarks<S: SystemOps>(
    root: &Path,
    backup_root: &Path,
    sys: &S,
    manifest: &mut Manifest,
) -> Result<(), EngineError> {
    let apps = root.join("Apps");
    for version in discover_versions(&apps) {
        let resources = version.path.join("Resources");
        if !resources.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&resources) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_watermark = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(WATERMARK_NAME));
            if !is_watermark || !path.is_file() {
                continue;
            }
            sys.set_readonly(&path, false).ok();
            let backup_rel = ensure_backup(&path, root, backup_root)?;
            fs::write(&path, TRANSPARENT_WATERMARK)?;
            sys.set_readonly(&path, true)?;
            manifest.push(ChangeEntry {
                rel_path: rel_string(root, &path),
                action: Action::ReplacedContent,
                original_sha256: None,
                backup_rel: Some(backup_rel),
            });
        }
    }
    Ok(())
}

/// Undo a block, strictly following the manifest.
pub fn restore<S: SystemOps>(root: &Path, backup_root: &Path, sys: &S) -> Result<(), EngineError> {
    if sys.process_running(PROCESS_IMAGE) {
        return Err(EngineError::CapCutRunning);
    }
    let manifest_path = backup_root.join(MANIFEST_NAME);
    if !manifest_path.exists() {
        return Err(EngineError::NoManifest);
    }
    let manifest = Manifest::from_json(&fs::read_to_string(&manifest_path)?)?;

    // Reverse order, so later changes are undone before earlier ones.
    for entry in manifest.entries.iter().rev() {
        let target = root.join(&entry.rel_path);
        // Always clear read-only first; harmless if it wasn't set.
        sys.set_readonly(&target, false).ok();

        match entry.action {
            Action::LockedReadonly => { /* clearing read-only above was the whole undo */ }
            Action::ReplacedContent => {
                if let Some(bak_rel) = &entry.backup_rel {
                    let bak = backup_root.join(bak_rel);
                    if bak.exists() {
                        fs::copy(&bak, &target)?;
                    }
                }
            }
            Action::CreatedPlaceholder => {
                if target.exists() {
                    fs::remove_file(&target).ok();
                }
            }
            Action::MovedAside => {
                if let Some(bak_rel) = &entry.backup_rel {
                    let bak = backup_root.join(bak_rel);
                    if bak.exists() {
                        fs::rename(&bak, &target)?;
                    }
                }
            }
            Action::AclDenyWrite => {
                // Lift the Deny entry. Because these are recorded last, this runs
                // before the file's content entry, so the later delete/overwrite
                // is no longer blocked by the Deny.
                sys.set_deny_write(&target, false)?;
            }
        }
    }

    fs::remove_file(&manifest_path).ok();
    Ok(())
}

/// Opt-in stronger lock: add a Deny-Write ACL on each frozen file that exists.
/// Entries are pushed LAST so reverse-order restore removes every Deny before
/// any content is deleted or overwritten (a Deny would otherwise block that).
fn harden_with_acl<S: SystemOps>(
    root: &Path,
    sys: &S,
    manifest: &mut Manifest,
) -> Result<(), EngineError> {
    let apps = root.join("Apps");
    let download = root.join("User Data").join("Download");
    let targets = [
        apps.join("configure.ini"),
        apps.join("ProductInfo.xml"),
        apps.join("update.exe"),
        download.join("update.exe"),
    ];
    for path in targets {
        if path.exists() {
            sys.set_deny_write(&path, true)?;
            manifest.push(ChangeEntry {
                rel_path: rel_string(root, &path),
                action: Action::AclDenyWrite,
                original_sha256: None,
                backup_rel: None,
            });
        }
    }
    Ok(())
}

// --- internals -------------------------------------------------------------

fn discover_versions(apps: &Path) -> Vec<InstalledVersion> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(apps) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                        out.push(InstalledVersion::parse(name, p.clone()));
                    }
                }
            }
        }
    }
    out
}

fn rel_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Copy `src` to its mirrored backup location, once (never overwrite an existing
/// backup - the first original captured wins and is preserved).
fn ensure_backup(src: &Path, root: &Path, backup_root: &Path) -> io::Result<String> {
    let rel = PathBuf::from(rel_string(root, src));
    let dest = backup_path_for(backup_root, &rel);
    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, &dest)?;
    }
    Ok(rel.to_string_lossy().into_owned())
}

fn pin_configure_ini<S: SystemOps>(
    ini: &Path,
    root: &Path,
    backup_root: &Path,
    last_version: &str,
    sys: &S,
    manifest: &mut Manifest,
) -> Result<(), EngineError> {
    let rel = rel_string(root, ini);

    if ini.exists() {
        sys.set_readonly(ini, false).ok();
        let backup_rel = ensure_backup(ini, root, backup_root)?;

        let original = fs::read_to_string(ini).unwrap_or_default();
        let mut out = String::new();
        let mut replaced = false;
        for line in original.lines() {
            if line.trim_start().starts_with("last_version=") {
                out.push_str(&format!("last_version={last_version}\n"));
                replaced = true;
            } else {
                out.push_str(line);
                out.push('\n');
            }
        }
        if !replaced {
            out.push_str(&format!("last_version={last_version}\n"));
        }
        fs::write(ini, out)?;
        sys.set_readonly(ini, true)?;

        manifest.push(ChangeEntry {
            rel_path: rel,
            action: Action::ReplacedContent,
            original_sha256: None, // populated once sha2 lands (download milestone)
            backup_rel: Some(backup_rel),
        });
    } else {
        if let Some(parent) = ini.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(ini, format!("[capcut]\nlast_version={last_version}\n"))?;
        sys.set_readonly(ini, true)?;

        manifest.push(ChangeEntry {
            rel_path: rel,
            action: Action::CreatedPlaceholder, // restore deletes what we created
            original_sha256: None,
            backup_rel: None,
        });
    }
    Ok(())
}

fn lock_readonly<S: SystemOps>(
    path: &Path,
    root: &Path,
    sys: &S,
    manifest: &mut Manifest,
) -> Result<(), EngineError> {
    sys.set_readonly(path, true)?;
    manifest.push(ChangeEntry {
        rel_path: rel_string(root, path),
        action: Action::LockedReadonly,
        original_sha256: None,
        backup_rel: None,
    });
    Ok(())
}

fn block_updater<S: SystemOps>(
    exe: &Path,
    root: &Path,
    backup_root: &Path,
    sys: &S,
    manifest: &mut Manifest,
) -> Result<(), EngineError> {
    let rel = rel_string(root, exe);

    if exe.exists() {
        sys.set_readonly(exe, false).ok();
        let backup_rel = ensure_backup(exe, root, backup_root)?;
        fs::write(exe, b"")?; // truncate the real updater to a 0-byte stub
        sys.set_readonly(exe, true)?;

        manifest.push(ChangeEntry {
            rel_path: rel,
            action: Action::ReplacedContent,
            original_sha256: None,
            backup_rel: Some(backup_rel),
        });
    } else {
        if let Some(parent) = exe.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(exe, b"")?; // occupy the path so the updater can't drop its own
        sys.set_readonly(exe, true)?;

        manifest.push(ChangeEntry {
            rel_path: rel,
            action: Action::CreatedPlaceholder,
            original_sha256: None,
            backup_rel: None,
        });
    }
    Ok(())
}

/// Dependency-free UTC timestamp (civil-from-days), so the manifest records a
/// real time without pulling a date crate.
fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (tod / 3600, (tod % 3600) / 60, tod % 60);

    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::MockSystem;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_root(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "capcut-blocker-test-{tag}-{}-{nanos}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// Build a fake install tree and return (root, backup_root).
    fn fake_install(tag: &str) -> (PathBuf, PathBuf) {
        let base = temp_root(tag);
        let root = base.join("CapCut");
        let apps = root.join("Apps");
        let dl = root.join("User Data").join("Download");
        fs::create_dir_all(&apps).unwrap();
        fs::create_dir_all(&dl).unwrap();
        fs::create_dir_all(apps.join("1.5.0")).unwrap();
        fs::create_dir_all(apps.join("8.2.0")).unwrap();

        fs::write(
            apps.join("configure.ini"),
            "[capcut]\nlast_version=9.9.9.9\n",
        )
        .unwrap();
        fs::write(apps.join("ProductInfo.xml"), "<product/>").unwrap();
        fs::write(apps.join("update.exe"), b"APPS-UPDATER-BINARY").unwrap();
        fs::write(dl.join("update.exe"), b"DOWNLOAD-UPDATER-BINARY").unwrap();

        let backup_root = base.join("backups");
        (root, backup_root)
    }

    #[test]
    fn block_then_restore_is_a_clean_round_trip() {
        let (root, backup_root) = fake_install("roundtrip");
        let sys = MockSystem::default();
        let apps = root.join("Apps");
        let dl = root.join("User Data").join("Download");

        let cfg = BlockConfig {
            pin_version: Some("1.5.0"),
            pinned_last_version: "1.0.0.0",
            harden_acl: false,
            remove_watermark: false,
        };
        let manifest = block(&root, &backup_root, &sys, &cfg).unwrap();

        // Pinned the requested version, not the highest.
        assert_eq!(manifest.capcut_version, "1.5.0");
        // configure.ini frozen.
        let ini = fs::read_to_string(apps.join("configure.ini")).unwrap();
        assert!(ini.contains("last_version=1.0.0.0"));
        assert!(!ini.contains("9.9.9.9"));
        // Both updaters neutralized to 0 bytes.
        assert_eq!(fs::metadata(apps.join("update.exe")).unwrap().len(), 0);
        assert_eq!(fs::metadata(dl.join("update.exe")).unwrap().len(), 0);
        // Old version folders left untouched (fix #1).
        assert!(apps.join("1.5.0").is_dir());
        assert!(apps.join("8.2.0").is_dir());
        // Manifest + read-only state recorded.
        assert!(is_blocked(&backup_root));
        assert!(sys.is_readonly(&apps.join("update.exe")).unwrap());

        // The two update.exe backups must be DISTINCT files (fix #2).
        let apps_bak = backup_root.join("Apps").join("update.exe");
        let dl_bak = backup_root
            .join("User Data")
            .join("Download")
            .join("update.exe");
        assert_ne!(apps_bak, dl_bak);
        assert_eq!(fs::read(&apps_bak).unwrap(), b"APPS-UPDATER-BINARY");
        assert_eq!(fs::read(&dl_bak).unwrap(), b"DOWNLOAD-UPDATER-BINARY");

        // Restore puts the RIGHT binary back in each location.
        restore(&root, &backup_root, &sys).unwrap();
        assert_eq!(
            fs::read(apps.join("update.exe")).unwrap(),
            b"APPS-UPDATER-BINARY"
        );
        assert_eq!(
            fs::read(dl.join("update.exe")).unwrap(),
            b"DOWNLOAD-UPDATER-BINARY"
        );
        let ini = fs::read_to_string(apps.join("configure.ini")).unwrap();
        assert!(ini.contains("9.9.9.9")); // original content back
        assert!(!is_blocked(&backup_root)); // manifest consumed

        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn refuses_when_capcut_running() {
        let (root, backup_root) = fake_install("running");
        let sys = MockSystem::default();
        sys.set_running(true);
        let err = block(&root, &backup_root, &sys, &BlockConfig::default()).unwrap_err();
        assert!(matches!(err, EngineError::CapCutRunning));
        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn restore_without_manifest_errors() {
        let (root, backup_root) = fake_install("nomanifest");
        let sys = MockSystem::default();
        let err = restore(&root, &backup_root, &sys).unwrap_err();
        assert!(matches!(err, EngineError::NoManifest));
        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn creates_and_removes_placeholder_when_updater_absent() {
        // No Download/update.exe on disk: block creates a placeholder, restore deletes it.
        let base = temp_root("placeholder");
        let root = base.join("CapCut");
        let apps = root.join("Apps");
        fs::create_dir_all(&apps).unwrap();
        fs::write(apps.join("configure.ini"), "last_version=5.0\n").unwrap();
        let backup_root = base.join("backups");
        let sys = MockSystem::default();

        block(&root, &backup_root, &sys, &BlockConfig::default()).unwrap();
        let dl_exe = root.join("User Data").join("Download").join("update.exe");
        assert!(dl_exe.exists());
        assert_eq!(fs::metadata(&dl_exe).unwrap().len(), 0);

        restore(&root, &backup_root, &sys).unwrap();
        assert!(
            !dl_exe.exists(),
            "placeholder we created must be gone after restore"
        );

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn acl_hardening_is_off_by_default() {
        let (root, backup_root) = fake_install("acl-off");
        let sys = MockSystem::default();
        let m = block(&root, &backup_root, &sys, &BlockConfig::default()).unwrap();
        assert!(
            !m.entries
                .iter()
                .any(|e| matches!(e.action, Action::AclDenyWrite)),
            "default config must not record any ACL change"
        );
        assert!(!sys.is_deny_write(&root.join("Apps").join("update.exe")));
        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn acl_hardening_opt_in_is_applied_and_reversed() {
        let (root, backup_root) = fake_install("acl-on");
        let sys = MockSystem::default();
        let apps = root.join("Apps");

        let cfg = BlockConfig {
            harden_acl: true,
            ..BlockConfig::default()
        };
        let m = block(&root, &backup_root, &sys, &cfg).unwrap();

        // Deny-Write entries recorded, and they come LAST in the manifest so
        // reverse-order restore lifts them before touching content.
        let acl_entries = m
            .entries
            .iter()
            .filter(|e| matches!(e.action, Action::AclDenyWrite))
            .count();
        assert!(
            acl_entries >= 2,
            "expected >=2 deny-write entries, got {acl_entries}"
        );
        assert!(matches!(
            m.entries.last().map(|e| &e.action),
            Some(Action::AclDenyWrite)
        ));
        assert!(sys.is_deny_write(&apps.join("update.exe")));
        assert!(sys.is_deny_write(&apps.join("configure.ini")));

        restore(&root, &backup_root, &sys).unwrap();
        assert!(!sys.is_deny_write(&apps.join("update.exe")));
        assert!(!sys.is_deny_write(&apps.join("configure.ini")));
        // Content still correctly restored with the ACL in play.
        let ini = fs::read_to_string(apps.join("configure.ini")).unwrap();
        assert!(ini.contains("9.9.9.9"));

        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn watermark_is_replaced_and_restored() {
        let (root, backup_root) = fake_install("watermark");
        let res = root.join("Apps").join("1.5.0").join("Resources");
        fs::create_dir_all(&res).unwrap();
        let wm = res.join("CClogo-w.png");
        fs::write(&wm, b"ORIGINAL-CAPCUT-WATERMARK").unwrap();
        let sys = MockSystem::default();

        let cfg = BlockConfig {
            remove_watermark: true,
            ..BlockConfig::default()
        };
        let m = block(&root, &backup_root, &sys, &cfg).unwrap();

        // Swapped for the embedded transparent image and recorded.
        assert_eq!(fs::read(&wm).unwrap(), TRANSPARENT_WATERMARK);
        assert!(m
            .entries
            .iter()
            .any(|e| e.rel_path.ends_with("CClogo-w.png")
                && matches!(e.action, Action::ReplacedContent)));
        // Original backed up under its mirrored path.
        let bak = backup_root
            .join("Apps")
            .join("1.5.0")
            .join("Resources")
            .join("CClogo-w.png");
        assert_eq!(fs::read(&bak).unwrap(), b"ORIGINAL-CAPCUT-WATERMARK");

        // Restore brings the original watermark back - no special-casing.
        restore(&root, &backup_root, &sys).unwrap();
        assert_eq!(fs::read(&wm).unwrap(), b"ORIGINAL-CAPCUT-WATERMARK");

        fs::remove_dir_all(root.parent().unwrap()).ok();
    }

    #[test]
    fn watermark_left_alone_when_disabled() {
        let (root, backup_root) = fake_install("watermark-off");
        let res = root.join("Apps").join("1.5.0").join("Resources");
        fs::create_dir_all(&res).unwrap();
        let wm = res.join("CClogo-w.png");
        fs::write(&wm, b"ORIGINAL").unwrap();
        let sys = MockSystem::default();

        // Default config leaves remove_watermark off.
        block(&root, &backup_root, &sys, &BlockConfig::default()).unwrap();
        assert_eq!(fs::read(&wm).unwrap(), b"ORIGINAL");

        fs::remove_dir_all(root.parent().unwrap()).ok();
    }
}
