//! Installer downloader: streamed download with progress, cancel, and optional
//! SHA-256 integrity verification.
//!
//! The URLs are the OFFICIAL CapCut installers on ByteDance's own CDN - this
//! just fetches the vendor's installer, the same bytes you'd get from their
//! site. The version list lives here as the single source of truth; the UI
//! derives its dropdown from it (fixing the Python README/code version drift).

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

/// One downloadable installer build.
pub struct VersionEntry {
    pub label: &'static str,
    pub url: &'static str,
    /// Pinned SHA-256 (lowercase hex). `None` = not pinned yet: the download
    /// still succeeds but is reported as *unverified*. Fill these in once you
    /// have a trusted hash of each installer and integrity is enforced for free.
    pub sha256: Option<&'static str>,
}

/// Single source of truth for installer versions (slice order = UI order).
pub const VERSIONS: &[VersionEntry] = &[
    VersionEntry {
        label: "v1.0.5 (Ultra Legacy)",
        url: "https://sf16-web-tos-buz.capcutcdn-us.com/obj/capcut-web-buz-tx/packages/CapCut_1_0_5_80_capcutpc_0.exe",
        sha256: None,
    },
    VersionEntry {
        label: "v1.5.0 (Sweet Spot)",
        url: "https://sf16-web-tos-buz.capcutcdn-us.com/obj/capcut-web-buz-tx/packages/CapCut_1_5_0_230_capcutpc_0.exe",
        sha256: None,
    },
    VersionEntry {
        label: "v2.0.0 (Free Features Stable)",
        url: "https://sf16-web-tos-buz.capcutcdn-us.com/obj/capcut-web-buz-tx/packages/CapCut_2_0_0_357_capcutpc_0.exe",
        sha256: None,
    },
    VersionEntry {
        label: "v7.0.0",
        url: "https://sf16-web-tos-buz.capcutcdn-us.com/obj/capcut-web-buz-tx/packages/CapCut_7_0_0_2865_capcutpc_0_creatortool.exe",
        sha256: None,
    },
    VersionEntry {
        label: "v8.2.0 (Latest Supported)",
        url: "https://sf16-web-tos-buz.capcutcdn-us.com/obj/capcut-web-buz-tx/packages/CapCut_8_2_0_3462_capcutpc_0_creatortool.exe",
        sha256: None,
    },
];

/// Minimum plausible installer size; anything smaller is treated as a failed or
/// redirected download (mirrors the Python "> 1 MB" sanity check).
const MIN_INSTALLER_BYTES: u64 = 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("download cancelled")]
    Cancelled,
    #[error("network error: {0}")]
    Http(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("integrity check failed - expected {expected}, got {actual}")]
    IntegrityMismatch { expected: String, actual: String },
    #[error("downloaded file is implausibly small ({0} bytes) - treating as failed")]
    TooSmall(u64),
}

/// Whether the finished download was integrity-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    /// SHA-256 matched the pinned hash.
    Verified,
    /// No hash was pinned for this version; integrity was not checked.
    Unverified,
}

/// Download `url` to `dest`, streaming via a `.part` file. Reports progress in
/// `[0.0, 1.0]`, aborts promptly when `cancel` is set (removing the partial
/// file), and - when a hash is pinned - verifies SHA-256 before moving the file
/// into place. Hashing happens in the same streaming pass, so it is free.
pub fn download(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f32),
) -> Result<Verification, DownloadError> {
    use ureq::config::Config;
    use ureq::tls::{TlsConfig, TlsProvider};

    // Explicitly select native-tls (SChannel on Windows, OpenSSL on Linux) so
    // the default provider choice can't silently require a backend we didn't
    // compile.
    let agent = Config::builder()
        .tls_config(
            TlsConfig::builder()
                .provider(TlsProvider::NativeTls)
                .build(),
        )
        .build()
        .new_agent();

    let mut resp = agent
        .get(url)
        .call()
        .map_err(|e| DownloadError::Http(e.to_string()))?;

    let total: u64 = resp
        .headers()
        .get(ureq::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let part = part_path(dest);
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emitted = -1.0f32;

    {
        let mut reader = resp.body_mut().as_reader();
        let mut file = std::fs::File::create(&part)?;
        let mut buf = [0u8; 128 * 1024];

        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = std::fs::remove_file(&part);
                return Err(DownloadError::Cancelled);
            }
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            downloaded += n as u64;

            if total > 0 {
                let frac = (downloaded as f32 / total as f32).clamp(0.0, 1.0);
                if frac - last_emitted >= 0.01 {
                    on_progress(frac);
                    last_emitted = frac;
                }
            }
        }
        file.flush()?;
    }

    if downloaded < MIN_INSTALLER_BYTES {
        let _ = std::fs::remove_file(&part);
        return Err(DownloadError::TooSmall(downloaded));
    }

    let actual = hex_lower(hasher.finalize());
    let verification = match expected_sha256 {
        Some(expected) if !expected.eq_ignore_ascii_case(&actual) => {
            let _ = std::fs::remove_file(&part);
            return Err(DownloadError::IntegrityMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
        Some(_) => Verification::Verified,
        None => Verification::Unverified,
    };

    if dest.exists() {
        std::fs::remove_file(dest)?;
    }
    std::fs::rename(&part, dest)?;
    on_progress(1.0);

    Ok(verification)
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_os_string();
    s.push(".part");
    PathBuf::from(s)
}

/// Lowercase hex, without an external hex crate.
fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_lower_of_empty_sha256() {
        assert_eq!(
            hex_lower(Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hex_lower_of_abc_sha256() {
        assert_eq!(
            hex_lower(Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/tmp/foo.exe")),
            Path::new("/tmp/foo.exe.part")
        );
    }

    #[test]
    fn versions_are_https_and_labels_unique() {
        assert!(!VERSIONS.is_empty());
        for v in VERSIONS {
            assert!(v.url.starts_with("https://"), "{} is not https", v.label);
        }
        for (i, a) in VERSIONS.iter().enumerate() {
            for b in &VERSIONS[i + 1..] {
                assert_ne!(a.label, b.label, "duplicate label");
            }
        }
    }
}
