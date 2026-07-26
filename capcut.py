import os
import sys
import subprocess
import time
import ctypes
import threading
import tkinter as tk
from tkinter import ttk, messagebox, scrolledtext, font
import json
import shutil
import stat
from datetime import datetime
from pathlib import Path
import webbrowser

# --- Core Helper Functions ---

def is_admin():
    """Check if script is running with admin privileges"""
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except:
        return False

def run_as_admin():
    """Relaunch script with admin privileges"""
    if sys.platform == 'win32':
        result = ctypes.windll.shell32.ShellExecuteW(
            None, "runas", sys.executable, " ".join(sys.argv), None, 1
        )
        if result <= 32:
            ctypes.windll.user32.MessageBoxW(
                None, 
                "Administrator privileges are required to lock CapCut configuration files. The application will now close.", 
                "Administrator Required", 
                0x30
            )

def get_capcut_path():
    """Get CapCut installation path"""
    localappdata = os.getenv('LOCALAPPDATA')
    if not localappdata:
        return Path.home() / "AppData" / "Local" / "CapCut"
    return Path(localappdata) / "CapCut"

class CapCutBlockerApp:
    def __init__(self, root):
        self.root = root
        self.root.title("CapCut Update Blocker")
        self.root.geometry("500x700")
        self.root.resizable(True, True)
        
        # Color & Font Configuration (Modern Dark Mode)
        self.bg_color = "#121214"          # Dark Obsidian background
        self.card_bg = "#1A1A1D"           # Card frame background
        self.text_primary = "#FFFFFF"      # Crisp white text
        self.text_dim = "#8E8E93"          # Muted gray text
        self.border_color = "#2A2A2E"      # Thin border line
        
        # Action button colors
        self.accent_color = "#007AFF"      # Accent Blue
        self.accent_hover = "#0A84FF"      # Hover state Blue
        
        self.root.configure(bg=self.bg_color)
        
        self.header_font = font.Font(family="Segoe UI", size=16, weight="bold")
        self.normal_font = font.Font(family="Segoe UI", size=10)
        self.button_font = font.Font(family="Segoe UI", size=10, weight="bold")
        self.mono_font = font.Font(family="Consolas", size=9)

        # Version Map
        self.versions = {
            "v1.0.5 (Ultra Legacy)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_1_0_5_80_capcutpc_0.exe",
            "v1.5.0": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_1_5_0_230_capcutpc_0.exe",
            "v2.0.0 (Legacy Stable)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_2_0_0_357_capcutpc_0.exe",
            "v3.0.0 (Split)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_split_3_0_0_1015_capcutpc_0.exe",
            "v4.0.0 (Stable)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_4_0_0_1539_capcutpc_0_creatortool.exe",
            "v5.0.0 (Latest Supported)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_5_0_0_1886_capcutpc_0_creatortool.exe"
        }

        style = ttk.Style()
        style.theme_use('clam')
        style.configure("TFrame", background=self.bg_color)
        style.configure("TLabel", background=self.bg_color, foreground=self.text_primary, font=self.normal_font)
        
        # Card style (LabelFrame)
        style.configure("TLabelframe", background=self.card_bg, bordercolor=self.border_color, borderwidth=1, relief="flat")
        style.configure("TLabelframe.Label", background=self.card_bg, foreground=self.text_primary, font=('Segoe UI', 10, 'bold'))
        
        # Primary buttons styling
        style.configure("TButton",
                        font=self.button_font,
                        background=self.accent_color,
                        foreground="#FFFFFF",
                        borderwidth=0,
                        focuscolor="none",
                        padding=(10, 8))
        style.map("TButton",
                  background=[("active", self.accent_hover), ("disabled", "#2C2C2E")],
                  foreground=[("disabled", "#555558")])
                  
        # Secondary buttons styling (e.g. Restore, Support, Cancel)
        style.configure("Secondary.TButton",
                        font=self.button_font,
                        background="#2C2C2E",
                        foreground=self.text_primary,
                        borderwidth=0,
                        focuscolor="none",
                        padding=(10, 8))
        style.map("Secondary.TButton",
                  background=[("active", "#3A3A3C"), ("disabled", "#1E1E20")],
                  foreground=[("disabled", "#555558")])
                  
        # Combobox customization
        style.configure("TCombobox",
                        fieldbackground=self.card_bg,
                        background=self.card_bg,
                        foreground=self.text_primary,
                        bordercolor=self.border_color,
                        arrowcolor=self.text_primary,
                        arrowsize=12)
        style.map("TCombobox",
                  fieldbackground=[("readonly", self.card_bg)],
                  selectbackground=[("readonly", self.accent_color)],
                  selectforeground=[("readonly", "#FFFFFF")])
                  
        # Option mappings for combobox listbox
        self.root.option_add("*TCombobox*Listbox.background", self.card_bg)
        self.root.option_add("*TCombobox*Listbox.foreground", self.text_primary)
        self.root.option_add("*TCombobox*Listbox.selectBackground", self.accent_color)
        self.root.option_add("*TCombobox*Listbox.selectForeground", "#FFFFFF")
        self.root.option_add("*TCombobox*Listbox.font", self.normal_font)
        self.root.option_add("*TCombobox*Listbox.borderWidth", 0)
        self.root.option_add("*TCombobox*Listbox.highlightThickness", 0)
        
        # Scrollbar customization
        style.configure("Vertical.TScrollbar",
                        background="#2C2C2E",
                        troughcolor=self.bg_color,
                        bordercolor=self.bg_color,
                        arrowcolor=self.text_dim,
                        arrowsize=0,
                        width=10)
        style.map("Vertical.TScrollbar",
                  background=[("active", "#3A3A3C")])

        # Progressbar styling
        style.configure("TProgressbar",
                        thickness=6,
                        troughcolor="#1A1A1D",
                        background=self.accent_color,
                        bordercolor=self.border_color,
                        lightcolor=self.accent_color,
                        darkcolor=self.accent_color)
                        
        # Separator styling
        style.configure("TSeparator", background=self.border_color)

        # ---- UI Layout ----
        
        # 1. Header Area
        header_frame = ttk.Frame(root, padding="30 30 30 10")
        header_frame.pack(fill=tk.X)
        
        title_lbl = ttk.Label(header_frame, text="CapCut Update Blocker", font=self.header_font)
        title_lbl.pack(anchor=tk.W)
        
        subtitle_lbl = ttk.Label(header_frame, text="Lock version configuration to prevent forced updates.", font=self.normal_font, foreground=self.text_dim)
        subtitle_lbl.pack(anchor=tk.W, pady=(2, 0))

        # 2. Main Content Area
        main_content = ttk.Frame(root, padding="30 20 30 10")
        main_content.pack(fill=tk.BOTH, expand=False)

        # --- Status Section (Styled Card) ---
        self.status_frame = tk.Frame(main_content, bg="#1A1A1D", highlightthickness=1, highlightbackground=self.border_color, padx=15, pady=12)
        self.status_frame.pack(fill=tk.X, pady=(0, 20))
        
        self.status_dot = tk.Label(self.status_frame, text="●", font=('Segoe UI', 12), bg="#1A1A1D", fg="#8E8E93")
        self.status_dot.pack(side=tk.LEFT, padx=(0, 8))
        
        self.status_label = tk.Label(self.status_frame, text="Checking status...", font=('Segoe UI', 10, 'bold'), bg="#1A1A1D", fg="#E5E5EA")
        self.status_label.pack(side=tk.LEFT, fill=tk.X, expand=True, anchor=tk.W)

        # --- Protection Control Section ---
        controls_lf = ttk.LabelFrame(main_content, text="Protection Controls")
        controls_lf.pack(fill=tk.X, pady=(0, 20))

        self.btn_block = ttk.Button(controls_lf, text="🛡️  Block Updates", command=self.start_block_updates, style="TButton")
        self.btn_block.pack(fill=tk.X, pady=(0, 6))
        
        self.btn_restore = ttk.Button(controls_lf, text="🔓  Restore Original", command=self.start_restore, style="Secondary.TButton")
        self.btn_restore.pack(fill=tk.X, pady=(0, 6))
        
        self.btn_support = ttk.Button(controls_lf, text="☕  Support on Ko-Fi", command=self.open_support, style="Secondary.TButton")
        self.btn_support.pack(fill=tk.X)

        # --- Divider ---
        ttk.Separator(main_content, orient='horizontal').pack(fill=tk.X, pady=(0, 20))

        # --- Download Section ---
        download_lf = ttk.LabelFrame(main_content, text="Installer Downloader")
        download_lf.pack(fill=tk.X, pady=(0, 20))
        
        self.version_var = tk.StringVar()
        self.version_dropdown = ttk.Combobox(download_lf, textvariable=self.version_var, values=list(self.versions.keys()), state="readonly")
        self.version_dropdown.pack(fill=tk.X, pady=(0, 8))
        self.version_dropdown.current(1)

        self.btn_download = ttk.Button(download_lf, text="Download Installer", command=self.start_download, style="TButton")
        self.btn_download.pack(fill=tk.X)

        # --- Progress & Cancel (Hidden by default) ---
        self.progress_bar = ttk.Progressbar(download_lf, orient="horizontal", mode="determinate", style="TProgressbar")
        
        self.btn_cancel = ttk.Button(download_lf, text="Cancel Download", command=self.cancel_action, style="Secondary.TButton")

        # 3. Log Area (Fixed sizing)
        log_frame = ttk.LabelFrame(root, text="Activity Log")
        log_frame.pack(fill=tk.BOTH, expand=True, padx=30, pady=(0, 20))
        
        # Internal container for text + scrollbar
        log_inner = ttk.Frame(log_frame)
        log_inner.pack(fill=tk.BOTH, expand=True)
        
        self.log_area = tk.Text(log_inner, height=6, state='disabled', font=self.mono_font, relief=tk.FLAT, bg="#0E0E11", fg="#A1A1AA", insertbackground="#FFFFFF", highlightthickness=0)
        self.log_area.pack(side=tk.LEFT, fill=tk.BOTH, expand=True)
        
        self.log_area.tag_config("success", foreground="#30D158")
        self.log_area.tag_config("error", foreground="#FF453A")
        self.log_area.tag_config("warning", foreground="#FF9F0A")
        
        scrollbar = ttk.Scrollbar(log_inner, orient="vertical", command=self.log_area.yview)
        scrollbar.pack(side=tk.RIGHT, fill=tk.Y)
        
        self.log_area.configure(yscrollcommand=scrollbar.set)

        # Initial Logic
        self.refresh_status()

    def log(self, message):
        """Thread-safe logging"""
        self.log_area.config(state='normal')
        
        # Check message content for simple tag coloring
        if "✅" in message or "🎉" in message or "SUCCESS" in message:
            self.log_area.insert(tk.END, message + "\n", "success")
        elif "❌" in message or "Critical Error" in message or "failed" in message.lower():
            self.log_area.insert(tk.END, message + "\n", "error")
        elif "⚠️" in message:
            self.log_area.insert(tk.END, message + "\n", "warning")
        else:
            self.log_area.insert(tk.END, message + "\n")
            
        self.log_area.see(tk.END)
        self.log_area.config(state='disabled')

    def refresh_status(self):
        """Update status label based on installation"""
        capcut_path = get_capcut_path()
        if capcut_path.exists():
            self.status_frame.config(bg="#152D1D", highlightbackground="#1C4D2E")
            self.status_dot.config(text="●", fg="#30D158", bg="#152D1D")
            self.status_label.config(text=f"CapCut active at: {capcut_path.name}", fg="#30D158", bg="#152D1D")
        else:
            self.status_frame.config(bg="#2D1515", highlightbackground="#4D1C1C")
            self.status_dot.config(text="●", fg="#FF453A", bg="#2D1515")
            self.status_label.config(text="CapCut not detected. Install to proceed.", fg="#FF453A", bg="#2D1515")

    def run_threaded(self, target):
        thread = threading.Thread(target=target)
        thread.daemon = True
        thread.start()

    def set_buttons_state(self, state):
        self.btn_block.config(state=state)
        self.btn_download.config(state=state)
        self.btn_restore.config(state=state)
        self.btn_support.config(state=state)
        self.version_dropdown.config(state="readonly" if state == "normal" else "disabled")

    # --- Actions ---

    def start_block_updates(self):
        self.set_buttons_state("disabled")
        self.run_threaded(self.do_block_logic)

    def do_block_logic(self):
        try:
            self.log("-" * 50)
            self.log("🚀 Starting blocking process...")
            
            if self.is_capcut_running():
                self.log("❌ Error: CapCut is still running.")
                messagebox.showwarning("CapCut is Running", "Please save your work and close CapCut before proceeding.")
                return

            capcut_path = get_capcut_path()
            if not capcut_path.exists():
                 # Create it if it doesn't exist (user might want to pre-block)
                 self.log(f"   Creating directory: {capcut_path}")
                 capcut_path.mkdir(parents=True, exist_ok=True)
            
            apps_path = capcut_path / "Apps"
            userdata_path = capcut_path / "User Data"
            
            apps_path.mkdir(exist_ok=True)
            userdata_path.mkdir(exist_ok=True)
            
            self.clean_old_versions(apps_path)
            self.clean_update_cache(userdata_path)
            self.lock_configure_ini(apps_path)
            self.block_productinfo_xml(apps_path)
            self.block_update_exe(userdata_path)
            self.block_apps_update_exe(apps_path) # Added per user feedback
            
            self.log("\n🔍 Verifying locks...")
            if self.verify_locks(capcut_path):
                self.log("\n🎉 SUCCESS! All locks are active.")
                messagebox.showinfo("Success", "CapCut updates have been successfully blocked!\n\nYou can now use your preferred version without forced updates.")
            else:
                self.log("\n⚠️ Warning: Some locks verified as missing.")
                messagebox.showwarning("Warning", "Some locks might not be in place. Please check the log.")
                
        except Exception as e:
            self.log(f"❌ Critical Error: {e}")
            messagebox.showerror("Error", str(e))
        finally:
            self.root.after(0, lambda: self.set_buttons_state("normal"))

    def open_support(self):
        try:
            webbrowser.open("https://ko-fi.com/kayz")
            self.log("☕ Opening support link: https://ko-fi.com/kayz")
        except Exception as e:
            self.log(f"❌ Error opening support link: {e}")

    def start_download(self):
        version_name = self.version_var.get()
        if messagebox.askyesno("Confirm Download", f"This will download the CapCut {version_name} installer.\n\nProceed?"):
            self.set_buttons_state("disabled")
            self.run_threaded(self.do_download_logic)
        
    def do_download_logic(self):
        try:
            version_name = self.version_var.get()
            download_url = self.versions[version_name]
            
            self.log("-" * 50)
            self.log(f"📥 Initiating Download for {version_name}...")
            
            # Extract simple filename from version string
            clean_name = version_name.split(' ')[0].replace('.', '_')
            downloads_dir = Path(os.path.expanduser("~")) / "Downloads"
            installer_path = downloads_dir / f"capcut_{clean_name}_installer.exe"
            
            success = self.download_file_native(download_url, str(installer_path))
            
            if success:
                 self.log("\n✅ Download successfully saved to Downloads folder.")
                 self.log(f"   Path: {installer_path}")
                 
                 # Open explorer to the file
                 subprocess.run(f'explorer /select,"{installer_path}"')
                 
                 messagebox.showinfo("Download Complete", f"Installer saved to your Downloads folder:\n\n{installer_path}\n\nPlease run it manually to install CapCut.")
            else:
                # The browser fallback logic is inside download_file_native, so if we are here and False, main error happened
                pass

        except Exception as e:
            self.log(f"❌ Error during download: {e}")
        finally:
             self.root.after(0, lambda: self.set_buttons_state("normal"))

    # --- Logic Implementations ---
    
    def is_capcut_running(self):
        self.log("🔍 Checking if CapCut is running...")
        try:
            output = subprocess.check_output(
                ["tasklist", "/FI", "IMAGENAME eq CapCut.exe", "/NH"], 
                creationflags=subprocess.CREATE_NO_WINDOW,
                text=True
            )
            return "CapCut.exe" in output
        except:
            return False

    def remove_readonly_error_handler(self, func, path, exc_info):
        """Error handler for shutil.rmtree to remove read-only attributes"""
        try:
            os.chmod(path, stat.S_IWRITE)
            func(path)
        except Exception:
            pass

    def get_dir_size(self, path):
        total = 0
        for p in Path(path).rglob('*'):
            if p.is_file():
                try: total += p.stat().st_size
                except: pass
        return total

    def download_file_native(self, url, save_path):
        """Native Python download with Progress & Cancel"""
        try:
            import urllib.request
            self.log(f"   Target: {Path(save_path).name}")
            self.log("   Method: Native Python Download...")
            
            # Setup UI for download
            self.cancel_download_flag = False
            self.root.after(0, lambda: self.show_download_ui(True))
            
            # Work with a temporary file
            temp_path = str(save_path) + ".part"
            
            headers = {'User-Agent': 'Mozilla/5.0'}
            req = urllib.request.Request(url, headers=headers)
            
            with urllib.request.urlopen(req) as response:
                total_size = int(response.info().get('Content-Length', 0))
                block_size = 131072 # 128KB chunks for faster downloading
                downloaded_size = 0
                
                with open(temp_path, 'wb') as file:
                    while True:
                        if self.cancel_download_flag:
                            self.log("   ⚠️ Download cancelled by user.")
                            file.close()
                            os.remove(temp_path)
                            return False
                        
                        buffer = response.read(block_size)
                        if not buffer:
                            break
                        
                        downloaded_size += len(buffer)
                        file.write(buffer)
                        
                        # Update progress
                        if total_size > 0:
                            percent = (downloaded_size / total_size) * 100
                            self.root.after(0, lambda p=percent: self.update_progress(p))

            # Move temp file to final path
            if os.path.exists(temp_path):
                if os.path.exists(save_path):
                    os.remove(save_path)
                os.rename(temp_path, save_path)
            
            if os.path.exists(save_path) and os.path.getsize(save_path) > 1000000:
                self.log("   ✅ Download successful.")
                return True
            else:
                self.log("   ⚠️ Download finished but file seems too small.")
                if os.path.exists(save_path): os.remove(save_path) # Cleanup
                return False

        except Exception as e:
            self.log(f"   ⚠️ Native download error: {e}")
            if 'temp_path' in locals() and os.path.exists(temp_path):
                try: os.remove(temp_path)
                except: pass
            return False
        finally:
            self.root.after(0, lambda: self.show_download_ui(False))
        
        # Fallback only on error (not cancel)
        self.log("   Switching to Browser Fallback...")
        
        try:
            import webbrowser
            self.log("   Opening direct download link in default browser...")
            webbrowser.open(url)
            messagebox.showinfo("Browser Download", "Automated download failed.\n\nWe have opened the direct download link in your browser.\n\nPlease save the file, then install it manually.")
            return False 
        except:
            self.log("❌ Failed to open browser.")
            return False

    def show_download_ui(self, show):
        if show:
            self.progress_bar.pack(fill=tk.X, pady=(8, 6))
            self.btn_cancel.pack(fill=tk.X)
            self.btn_download.config(state='disabled')
            self.version_dropdown.config(state='disabled')
            self.btn_block.config(state='disabled')
            self.btn_restore.config(state='disabled')
            self.btn_support.config(state='disabled')
        else:
            self.progress_bar.stop()
            self.progress_bar.pack_forget()
            self.btn_cancel.pack_forget()
            self.btn_download.config(state='normal')
            self.version_dropdown.config(state='readonly')
            self.btn_block.config(state='normal')
            self.btn_restore.config(state='normal')
            self.btn_support.config(state='normal')

    def update_progress(self, percent):
        self.progress_bar['value'] = percent
        self.root.update_idletasks()

    def cancel_action(self):
        self.cancel_download_flag = True
        self.btn_cancel.config(state='disabled') # Prevent double clicks

    def clean_old_versions(self, apps_path):
        self.log("🧹 Cleaning update artifacts...")
        if not apps_path.exists(): return
        
        # Identify version folders (names like 1.5.0, 2.0.0)
        version_dirs = []
        for item in apps_path.iterdir():
            if item.is_dir() and item.name[0].isdigit():
                version_dirs.append(item)
        
        if not version_dirs:
            self.log("   No version folders found.")
            return

        # Sort by name (simple way to find 'latest')
        version_dirs.sort(key=lambda x: [int(p) for p in x.name.split('.') if p.isdigit()], reverse=True)
        
        # Protect the latest/active version, clean others
        active_version = version_dirs[0]
        self.log(f"   Protecting active version: {active_version.name}")
        
        total_freed = 0
        for item in version_dirs[1:]:
            try:
                size = self.get_dir_size(item)
                shutil.rmtree(item, onerror=self.remove_readonly_error_handler)
                total_freed += size
                self.log(f"   Deleted old version: {item.name}")
            except Exception as e:
                 self.log(f"   Error cleaning {item.name}: {e}")
                 
        if total_freed > 0:
            mb = total_freed / (1024 * 1024)
            self.log(f"   ✅ Cleaned {mb:.1f} MB of old versions.")

    # --- Restore & Reverse logic ---

    def get_backup_dir(self):
        return Path(os.getenv('LOCALAPPDATA')) / "CapCutUpdateBlocker" / "OriginalSettings"

    def is_file_blocked(self, file_path):
        """Check if file appears to be already blocked/modified"""
        try:
            if not file_path.exists(): return False
            
            # Check 1: File size (Blocked executables are usually 0 bytes or very small)
            if file_path.suffix.lower() == '.exe':
                # Real update.exe is usually > 10MB. 1MB is a safe lower bound.
                if file_path.stat().st_size < 1024 * 1024: 
                    return True
            
            # Check 2: Content checks for configure.ini
            if file_path.name.lower() == 'configure.ini':
                # If we can't read it, assume it might be locked/crypto, but let's try
                try:
                    with open(file_path, 'r', errors='ignore') as f:
                        content = f.read()
                        # If it has the blocked version string, it's ours
                        if 'last_version=1.0.0.0' in content:
                            return True
                except: pass

            # Check 3: ProductInfo.xml (Blocker creates empty file)
            if file_path.name.lower() == 'productinfo.xml':
                 if file_path.stat().st_size == 0: return True

            return False
        except:
            return False

    def backup_config(self, file_path):
        """Save original file before we modify it"""
        try:
            if not file_path.exists(): return

            # SAFETY CHECK: Don't backup if it looks like it's ALREADY blocked
            if self.is_file_blocked(file_path):
                self.log(f"   ⚠️ Skipping backup of {file_path.name} (appears already blocked)")
                return

            backup_dir = self.get_backup_dir()
            backup_dir.mkdir(parents=True, exist_ok=True)
            
            # Save as filename.bak in our special appdata folder
            dest = backup_dir / f"{file_path.name}.bak"
            if not dest.exists(): # Only backup the VERY first time
                shutil.copy2(file_path, dest)
                self.log(f"   Stored original: {file_path.name}")
        except: pass

    def remove_readonly(self, file_path):
        """Remove Windows read-only/system/hidden attributes and unlock the file."""
        try:
            subprocess.run(
                ["attrib", "-r", "-s", "-h", str(file_path)],
                capture_output=True,
                check=False,
            )
        except: pass
        try:
            os.chmod(file_path, 0o666)
        except: pass

    def start_restore(self):
        if messagebox.askyesno("Confirm Restore", "This will UNLOCK and RESTORE CapCut to its original state.\n\nAre you sure you want to reverse the blocker?"):
            self.set_buttons_state("disabled")
            self.run_threaded(self.do_restore_logic)

    def do_restore_logic(self):
        try:
            self.log("-" * 50)
            self.log("🔓 Reversing blocker...")
            
            if self.is_capcut_running():
                self.log("❌ Error: CapCut is still running.")
                messagebox.showwarning("CapCut is Running", "Please save your work and close CapCut before proceeding.")
                return

            capcut_path = get_capcut_path()
            if not capcut_path.exists():
                self.log("❌ Error: CapCut installation not found.")
                return
            
            apps_path = capcut_path / "Apps"
            dl_path = capcut_path / "User Data" / "Download"
            
            targets = [
                (apps_path / "configure.ini", "configure.ini"),
                (apps_path / "ProductInfo.xml", "ProductInfo.xml"),
                (apps_path / "update.exe", "Apps/update.exe"),
                (dl_path / "update.exe", "Download/update.exe")
            ]

            backup_dir = self.get_backup_dir()

            for fp, name in targets:
                if fp.exists():
                    self.log(f"   Processing: {name}")
                    try:
                        # Force unlock using attrib for Windows stubborn files
                        self.remove_readonly(fp)
                    except Exception as e:
                        self.log(f"   ⚠️ Could not unlock {name}: {e}")

                    # Try to restore from backup
                    bak_file = backup_dir / f"{fp.name}.bak"
                    
                    if bak_file.exists() and bak_file.stat().st_size > 0:
                        try:
                            shutil.copy2(bak_file, fp)
                            self.log(f"   ✅ Restored original: {name}")
                        except Exception as e:
                            self.log(f"   ❌ Restore failed for {name}: {e}")
                    else:
                        # Logic for determining if it's a dummy file we should delete
                        # If it's exactly 0 bytes (like our touched files), delete it
                        try:
                            if fp.stat().st_size == 0: 
                                 self.remove_readonly(fp)
                                 fp.unlink()
                                 self.log(f"   🗑️ Removed dummy file: {name}")
                            else:
                                 self.log(f"   🔓 Unlocked existing file: {name}")
                        except Exception as e:
                            self.log(f"   ❌ Delete failed for {name}: {e}")

            self.log("\n🎉 SUCCESS! Blocker has been reversed.")
            messagebox.showinfo("Success", "Blocker reversed.\n\nCapCut is now back to its default state.")
            
        except Exception as e:
            self.log(f"❌ Error during restore: {e}")
            messagebox.showerror("Error", str(e))
        finally:
            self.root.after(0, self.refresh_status)
            self.root.after(0, lambda: self.set_buttons_state("normal"))

    def clean_update_cache(self, userdata_path):
        self.log("🗑️ Cleaning update cache folders...")
        folders = ["Cache", "Shadow_Cache", "Smart_Crop", "update_cache"]
        total_freed = 0
        for f in folders:
            fp = userdata_path / f
            if fp.exists():
                try: 
                    size = self.get_dir_size(fp)
                    shutil.rmtree(fp, onerror=self.remove_readonly_error_handler)
                    total_freed += size
                except: pass
                
        if total_freed > 0:
            mb = total_freed / (1024 * 1024)
            self.log(f"   ✅ Cleaned {mb:.1f} MB of cache.")

    def lock_configure_ini(self, apps_path):
        self.log("🔒 Locking configure.ini...")
        ini_path = apps_path / "configure.ini"
        try:
            self.backup_config(ini_path) # Backup original
            if ini_path.exists():
                with open(ini_path, 'r') as f: lines = f.readlines()
                with open(ini_path, 'w') as f:
                    for line in lines:
                        if line.strip().startswith('last_version='): f.write('last_version=1.0.0.0\n')
                        else: f.write(line)
            else:
                with open(ini_path, 'w') as f:
                    f.write('[capcut]\nlast_version=1.0.0.0\n')
            os.chmod(ini_path, 0o444)
        except Exception as e: self.log(f"❌ Error: {e}")

    def block_productinfo_xml(self, apps_path):
        self.log("🛡️ Blocking ProductInfo.xml...")
        xml_path = apps_path / "ProductInfo.xml"
        try:
            self.backup_config(xml_path) # Backup original
            if xml_path.exists():
                os.chmod(xml_path, 0o666) # Ensure we can read/lock it
            else:
                # Only create if it doesn't exist at all
                xml_path.touch()
            
            os.chmod(xml_path, 0o444) # SET TO READ-ONLY BUT DO NOT DELETE CONTENTS
        except Exception as e: self.log(f"❌ Error: {e}")

    def block_update_exe(self, userdata_path):
        self.log("⛔ Blocking Download/update.exe...")
        dl_path = userdata_path / "Download"
        dl_path.mkdir(exist_ok=True)
        exe_path = dl_path / "update.exe"
        try:
            self.backup_config(exe_path) # Backup original
            if exe_path.exists():
                os.chmod(exe_path, 0o666)
                exe_path.unlink()
            exe_path.touch()
            # Force set Read-Only using attrib (stronger than os.chmod)
            try:
                subprocess.run(["attrib", "+r", str(exe_path)], capture_output=True, check=False)
            except: pass
            os.chmod(exe_path, 0o444)
            self.log("   Set to Read-Only (attrib +r).")
        except Exception as e: self.log(f"❌ Error: {e}")

    def block_apps_update_exe(self, apps_path):
        self.log("⛔ Blocking Apps/update.exe...")
        exe_path = apps_path / "update.exe"
        try:
            self.backup_config(exe_path) # Backup original
            if exe_path.exists():
                os.chmod(exe_path, 0o666)
                exe_path.unlink()
            exe_path.touch()
            try:
                subprocess.run(["attrib", "+r", str(exe_path)], capture_output=True, check=False)
            except: pass
            os.chmod(exe_path, 0o444)
            self.log("   Set to Read-Only (attrib +r).")
        except Exception as e: self.log(f"❌ Error: {e}")

    def verify_locks(self, capcut_path):
        apps_path = capcut_path / "Apps"
        dl_path = capcut_path / "User Data" / "Download"
        checks = [
            (apps_path / "configure.ini", "configure.ini"),
            (apps_path / "ProductInfo.xml", "ProductInfo.xml"),
            (dl_path / "update.exe", "Download/update.exe"),
            (apps_path / "update.exe", "Apps/update.exe")
        ]
        all_good = True
        for fp, name in checks:
            status = "MISSING"
            if fp.exists():
                if "update.exe" in name:
                     if not os.access(fp, os.W_OK):
                         status = "Blocked"
                     else:
                         status = "Unblocked"
                         all_good = False
                elif not os.access(fp, os.W_OK): 
                     status = "Locked"
                else: 
                     status = "Unlocked"
                     all_good = False
            else:
                all_good = False
            
            symbol = "✅" if status in ["Locked", "Blocked"] else "❌"
            self.log(f"   {symbol} {name}: {status}")
            
        return all_good

if __name__ == "__main__":
    if not is_admin():
        # Re-run as admin if needed
        run_as_admin()
        sys.exit()
        
    root = tk.Tk()
    app = CapCutBlockerApp(root)
    root.mainloop()
