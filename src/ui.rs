//! egui front-end.
//!
//! The thread model that fixes the Python tool's unsafe UI: a single worker
//! thread owns the `SystemOps` impl and runs the engine; the UI talks to it over
//! channels and only ever renders state. No OS work and no shared mutable state
//! happen on the UI thread, and nothing but the worker touches the engine.

use capcut_blocker::app::download::{self, Verification, VERSIONS};
use capcut_blocker::app::engine::{self, BlockConfig};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
enum Level {
    Info,
    Success,
    Warn,
    Error,
}

/// UI -> worker.
enum Command {
    Block {
        pin: Option<String>,
        harden_acl: bool,
        remove_watermark: bool,
    },
    Restore,
    Download {
        index: usize,
    },
}

/// Worker -> UI.
enum Event {
    Log(String, Level),
    Progress(f32),
    Finished,
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 720.0])
            .with_min_inner_size([440.0, 600.0])
            .with_title("CapCut Update Blocker"),
        ..Default::default()
    };
    eframe::run_native(
        "CapCut Update Blocker",
        options,
        Box::new(|cc| Ok(Box::new(BlockerApp::new(cc)))),
    )
}

struct BlockerApp {
    cmd_tx: Sender<Command>,
    evt_rx: Receiver<Event>,
    root: PathBuf,
    backup_root: PathBuf,
    log: Vec<(String, Level)>,
    busy: bool,
    installed: bool,
    blocked: bool,
    pin_choices: Vec<String>,
    pin: Option<String>,
    harden_acl: bool,
    remove_watermark: bool,
    download_sel: usize,
    downloading: bool,
    progress: f32,
    cancel_flag: Arc<AtomicBool>,
    last_poll: Instant,
}

impl BlockerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = egui::Color32::from_rgb(0x12, 0x12, 0x14);
        visuals.window_fill = egui::Color32::from_rgb(0x1A, 0x1A, 0x1D);
        visuals.selection.bg_fill = egui::Color32::from_rgb(0x00, 0x7A, 0xFF);
        cc.egui_ctx.set_visuals(visuals);

        let root = capcut_root();
        let backup_root = backup_root_path();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let (cmd_tx, evt_rx) = spawn_worker(
            root.clone(),
            backup_root.clone(),
            cc.egui_ctx.clone(),
            cancel_flag.clone(),
        );

        let mut app = BlockerApp {
            cmd_tx,
            evt_rx,
            root,
            backup_root,
            log: Vec::new(),
            busy: false,
            installed: false,
            blocked: false,
            pin_choices: Vec::new(),
            pin: None,
            harden_acl: false,
            remove_watermark: true,
            download_sel: 1,
            downloading: false,
            progress: 0.0,
            cancel_flag,
            last_poll: Instant::now() - Duration::from_secs(10),
        };
        app.poll_status();
        app
    }

    fn poll_status(&mut self) {
        self.installed = engine::is_capcut_installed(&self.root);
        self.blocked = engine::is_blocked(&self.backup_root);
        self.pin_choices = engine::installed_versions(&self.root);
        let still_valid = self
            .pin
            .as_ref()
            .is_some_and(|p| self.pin_choices.iter().any(|v| v == p));
        if !still_valid {
            self.pin = self.pin_choices.first().cloned();
        }
        self.last_poll = Instant::now();
    }
}

impl eframe::App for BlockerApp {
    // eframe 0.37 renders into a provided `Ui` (no `update(ctx)` / `CentralPanel`).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Drain worker events.
        while let Ok(ev) = self.evt_rx.try_recv() {
            match ev {
                Event::Log(msg, lvl) => self.log.push((msg, lvl)),
                Event::Progress(p) => self.progress = p,
                Event::Finished => {
                    self.busy = false;
                    self.downloading = false;
                    self.progress = 0.0;
                    self.poll_status();
                }
            }
        }
        // Keep status fresh even with no input.
        if self.last_poll.elapsed() >= Duration::from_millis(1500) {
            self.poll_status();
        }

        let dim = egui::Color32::from_rgb(0x8E, 0x8E, 0x93);
        let accent = egui::Color32::from_rgb(0x00, 0x7A, 0xFF);
        let card = egui::Color32::from_rgb(0x1A, 0x1A, 0x1D);

        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading("CapCut Update Blocker");
            ui.label(egui::RichText::new("Lock a version and stop forced updates.").color(dim));
            ui.add_space(12.0);

            // Status card.
            let (text, col) = if self.blocked {
                (
                    "Protection active - updates blocked",
                    egui::Color32::from_rgb(0x30, 0xD1, 0x58),
                )
            } else if self.installed {
                ("CapCut installed - not blocked", accent)
            } else {
                (
                    "CapCut not detected",
                    egui::Color32::from_rgb(0xFF, 0x45, 0x3A),
                )
            };
            egui::Frame::group(ui.style()).fill(card).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("●").color(col));
                    ui.label(egui::RichText::new(text).color(col).strong());
                });
            });
            ui.add_space(12.0);

            // Protection controls.
            ui.group(|ui| {
                ui.label(egui::RichText::new("Protection").strong());
                ui.add_space(6.0);

                if !self.pin_choices.is_empty() {
                    let choices = self.pin_choices.clone();
                    ui.horizontal(|ui| {
                        ui.label("Protect version:");
                        egui::ComboBox::from_id_salt("pin")
                            .selected_text(self.pin.clone().unwrap_or_else(|| "-".to_owned()))
                            .show_ui(ui, |ui| {
                                for v in &choices {
                                    ui.selectable_value(&mut self.pin, Some(v.clone()), v.as_str());
                                }
                            });
                    });
                    ui.add_space(6.0);
                }

                ui.checkbox(&mut self.remove_watermark, "Remove export watermark");
                ui.checkbox(
                    &mut self.harden_acl,
                    "Stronger lock (Deny-Write ACL) - experimental",
                );
                ui.label(
                    egui::RichText::new(
                        "Harder to bypass than read-only; removable only by admin (Restore does). \
                         Validate on Windows before relying on it.",
                    )
                    .color(dim)
                    .small(),
                );
                ui.add_space(6.0);

                let full = egui::vec2(ui.available_width(), 32.0);
                let block_btn = ui.add_enabled(
                    !self.busy,
                    egui::Button::new(
                        egui::RichText::new("🛡  Block Updates").color(egui::Color32::WHITE),
                    )
                    .fill(accent)
                    .min_size(full),
                );
                if block_btn.clicked() {
                    self.busy = true;
                    self.log
                        .push(("Dispatching block...".to_owned(), Level::Info));
                    self.cmd_tx
                        .send(Command::Block {
                            pin: self.pin.clone(),
                            harden_acl: self.harden_acl,
                            remove_watermark: self.remove_watermark,
                        })
                        .ok();
                }
                ui.add_space(4.0);

                let restore_btn = ui.add_enabled(
                    !self.busy && self.blocked,
                    egui::Button::new("🔓  Restore Original")
                        .min_size(egui::vec2(ui.available_width(), 30.0)),
                );
                if restore_btn.clicked() {
                    self.busy = true;
                    self.log
                        .push(("Dispatching restore...".to_owned(), Level::Info));
                    self.cmd_tx.send(Command::Restore).ok();
                }
                ui.add_space(4.0);

                if ui
                    .add(
                        egui::Button::new("☕  Support on Ko-fi")
                            .min_size(egui::vec2(ui.available_width(), 28.0)),
                    )
                    .clicked()
                {
                    ui.ctx()
                        .open_url(egui::OpenUrl::new_tab("https://ko-fi.com/kayz"));
                }
            });
            ui.add_space(12.0);

            // Installer downloader.
            ui.group(|ui| {
                ui.label(egui::RichText::new("Installer Downloader").strong());
                ui.add_space(6.0);

                let selected = VERSIONS
                    .get(self.download_sel)
                    .map(|v| v.label)
                    .unwrap_or("-");
                egui::ComboBox::from_id_salt("dl")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (i, v) in VERSIONS.iter().enumerate() {
                            ui.selectable_value(&mut self.download_sel, i, v.label);
                        }
                    });
                ui.add_space(6.0);

                if self.downloading {
                    ui.add(
                        egui::ProgressBar::new(self.progress)
                            .show_percentage()
                            .desired_height(10.0),
                    );
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new("Cancel Download")
                                .min_size(egui::vec2(ui.available_width(), 28.0)),
                        )
                        .clicked()
                    {
                        self.cancel_flag.store(true, Ordering::Relaxed);
                        self.log
                            .push(("Cancelling download...".to_owned(), Level::Warn));
                    }
                } else {
                    let dl_btn = ui.add_enabled(
                        !self.busy,
                        egui::Button::new("Download Installer")
                            .min_size(egui::vec2(ui.available_width(), 28.0)),
                    );
                    if dl_btn.clicked() {
                        let label = VERSIONS
                            .get(self.download_sel)
                            .map(|v| v.label)
                            .unwrap_or("-");
                        self.busy = true;
                        self.downloading = true;
                        self.progress = 0.0;
                        self.cancel_flag.store(false, Ordering::Relaxed);
                        self.log
                            .push((format!("Downloading {label}..."), Level::Info));
                        self.cmd_tx
                            .send(Command::Download {
                                index: self.download_sel,
                            })
                            .ok();
                    }
                }
            });
            ui.add_space(12.0);

            // Activity log.
            ui.label(egui::RichText::new("Activity Log").strong());
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(0x0E, 0x0E, 0x11))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(170.0)
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.log.is_empty() {
                                ui.label(
                                    egui::RichText::new("No activity yet.")
                                        .color(dim)
                                        .monospace(),
                                );
                            }
                            for (msg, lvl) in &self.log {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .color(level_color(*lvl))
                                        .monospace(),
                                );
                            }
                        });
                });

            // Indeterminate bar for block/restore; downloads show their own
            // percentage bar in the downloader group above.
            if self.busy && !self.downloading {
                ui.add_space(6.0);
                ui.add(
                    egui::ProgressBar::new(0.0)
                        .animate(true)
                        .desired_height(6.0),
                );
            }
        });

        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

fn level_color(l: Level) -> egui::Color32 {
    match l {
        Level::Info => egui::Color32::from_rgb(0xA1, 0xA1, 0xAA),
        Level::Success => egui::Color32::from_rgb(0x30, 0xD1, 0x58),
        Level::Warn => egui::Color32::from_rgb(0xFF, 0x9F, 0x0A),
        Level::Error => egui::Color32::from_rgb(0xFF, 0x45, 0x3A),
    }
}

fn spawn_worker(
    root: PathBuf,
    backup_root: PathBuf,
    ctx: egui::Context,
    cancel: Arc<AtomicBool>,
) -> (Sender<Command>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (evt_tx, evt_rx) = channel::<Event>();

    thread::spawn(move || {
        let sys = make_sys();
        while let Ok(cmd) = cmd_rx.recv() {
            match cmd {
                Command::Block {
                    pin,
                    harden_acl,
                    remove_watermark,
                } => {
                    let cfg = BlockConfig {
                        pin_version: pin.as_deref(),
                        pinned_last_version: "1.0.0.0",
                        harden_acl,
                        remove_watermark,
                    };
                    let _ = evt_tx.send(Event::Log("Starting block...".to_owned(), Level::Info));
                    match engine::block(&root, &backup_root, &sys, &cfg) {
                        Ok(m) => {
                            let pinned = if m.capcut_version.is_empty() {
                                "(highest installed)".to_owned()
                            } else {
                                m.capcut_version.clone()
                            };
                            let _ = evt_tx.send(Event::Log(
                                format!(
                                    "Blocked - {} change(s) recorded, protecting {pinned}.",
                                    m.entries.len()
                                ),
                                Level::Success,
                            ));
                        }
                        Err(e) => {
                            let _ =
                                evt_tx.send(Event::Log(format!("Block failed: {e}"), Level::Error));
                        }
                    }
                }
                Command::Restore => {
                    let _ = evt_tx.send(Event::Log("Starting restore...".to_owned(), Level::Info));
                    match engine::restore(&root, &backup_root, &sys) {
                        Ok(()) => {
                            let _ = evt_tx.send(Event::Log(
                                "Restore complete - originals returned.".to_owned(),
                                Level::Success,
                            ));
                        }
                        Err(e) => {
                            let _ = evt_tx
                                .send(Event::Log(format!("Restore failed: {e}"), Level::Error));
                        }
                    }
                }
                Command::Download { index } => {
                    if let Some(entry) = VERSIONS.get(index) {
                        let dest = downloads_dir().join(installer_filename(entry.label));
                        // Progress callback: forward fractions to the UI and nudge
                        // a repaint. Cancel is a shared flag the stream polls.
                        let progress = |p: f32| {
                            let _ = evt_tx.send(Event::Progress(p));
                            ctx.request_repaint();
                        };
                        match download::download(entry.url, &dest, entry.sha256, &cancel, progress)
                        {
                            Ok(Verification::Verified) => {
                                let _ = evt_tx.send(Event::Log(
                                    format!("Saved & SHA-256 verified -> {}", dest.display()),
                                    Level::Success,
                                ));
                            }
                            Ok(Verification::Unverified) => {
                                let _ = evt_tx.send(Event::Log(
                                    format!(
                                        "Saved -> {} (integrity not pinned for this version)",
                                        dest.display()
                                    ),
                                    Level::Warn,
                                ));
                            }
                            Err(download::DownloadError::Cancelled) => {
                                let _ = evt_tx.send(Event::Log(
                                    "Download cancelled.".to_owned(),
                                    Level::Warn,
                                ));
                            }
                            Err(e) => {
                                let _ = evt_tx.send(Event::Log(
                                    format!("Download failed: {e}"),
                                    Level::Error,
                                ));
                            }
                        }
                    }
                }
            }
            let _ = evt_tx.send(Event::Finished);
            ctx.request_repaint(); // nudge the UI to drain events promptly
        }
    });

    (cmd_tx, evt_rx)
}

#[cfg(windows)]
fn make_sys() -> capcut_blocker::platform::WindowsSystem {
    capcut_blocker::platform::WindowsSystem
}

#[cfg(not(windows))]
fn make_sys() -> capcut_blocker::platform::MockSystem {
    // Dev builds run with the in-memory fake, so the GUI can be exercised on
    // Linux without touching any real files.
    capcut_blocker::platform::MockSystem::default()
}

fn capcut_root() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(p).join("CapCut");
        }
        if let Some(p) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(p)
                .join("AppData")
                .join("Local")
                .join("CapCut");
        }
        PathBuf::from("CapCut")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir()
            .join("capcut-blocker-dev")
            .join("CapCut")
    }
}

fn backup_root_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(p).join("CapCutBlocker").join("backups");
        }
        PathBuf::from("CapCutBlocker").join("backups")
    }
    #[cfg(not(windows))]
    {
        std::env::temp_dir()
            .join("capcut-blocker-dev")
            .join("backups")
    }
}

/// Where downloaded installers are saved.
fn downloads_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("USERPROFILE") {
            return PathBuf::from(p).join("Downloads");
        }
        PathBuf::from("Downloads")
    }
    #[cfg(not(windows))]
    {
        // Keep dev downloads out of the real ~/Downloads.
        std::env::temp_dir()
            .join("capcut-blocker-dev")
            .join("downloads")
    }
}

/// A tidy installer filename from a version label, e.g.
/// "v1.5.0 (Sweet Spot)" -> "CapCut_v1_5_0_installer.exe".
fn installer_filename(label: &str) -> String {
    let token = label.split_whitespace().next().unwrap_or("latest");
    let slug: String = token
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("CapCut_{slug}_installer.exe")
}
