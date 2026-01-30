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
from datetime import datetime
from pathlib import Path

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
        ctypes.windll.shell32.ShellExecuteW(
            None, "runas", sys.executable, " ".join(sys.argv), None, 1
        )

def get_capcut_path():
    """Get CapCut installation path"""
    localappdata = os.getenv('LOCALAPPDATA')
    if not localappdata:
        return Path("C:/") 
    capcut_path = Path(localappdata) / "CapCut"
    return capcut_path

class CapCutBlockerApp:
    def __init__(self, root):
        self.root = root
        self.root.title("CapCut Update Blocker")
        self.root.geometry("500x700")
        self.root.resizable(True, True)
        self.root.configure(bg="#FFFFFF")
        
        # Color & Font Configuration
        self.accent_color = "#000000"  # Sleek Black
        self.bg_color = "#FFFFFF"
        self.text_dim = "#666666"
        
        self.header_font = font.Font(family="Segoe UI", size=16, weight="bold")
        self.normal_font = font.Font(family="Segoe UI", size=10)
        self.button_font = font.Font(family="Segoe UI", size=10, weight="bold")
        self.mono_font = font.Font(family="Consolas", size=9)

        # Version Map
        self.versions = {
            "v1.0.5 (Ultra Legacy)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_1_0_5_80_capcutpc_0.exe",
            "v2.0.0 (Legacy Stable)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_2_0_0_357_capcutpc_0.exe",
            "v3.0.0 (Split)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_split_3_0_0_1015_capcutpc_0.exe",
            "v4.0.0 (Stable)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_4_0_0_1539_capcutpc_0_creatortool.exe",
            "v5.0.0 (Latest Supported)": "https://lf16-capcut.faceulv.com/obj/capcutpc-packages-us/packages/CapCut_5_0_0_1886_capcutpc_0_creatortool.exe"
        }

        style = ttk.Style()
        style.theme_use('clam')
        style.configure("TFrame", background=self.bg_color)
        style.configure("TLabel", background=self.bg_color, font=self.normal_font)
        style.configure("TButton", font=self.button_font, padding=10)
        style.configure("TLabelframe", background=self.bg_color, padding=15)
        style.configure("TLabelframe.Label", background=self.bg_color, font=('Segoe UI', 10, 'bold'))

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

        # --- Status Section ---
        status_frame = ttk.Frame(main_content)
        status_frame.pack(fill=tk.X, pady=(0, 20))
        
        self.status_label = ttk.Label(status_frame, text="Checking status...", font=self.normal_font)
        self.status_label.pack(anchor=tk.W)

        # --- Protection Control Section ---
        controls_lf = ttk.LabelFrame(main_content, text="Protection Controls", padding="15")
        controls_lf.pack(fill=tk.X, pady=(0, 20))

        # Grid layout for buttons to keep them close
        self.btn_block = ttk.Button(controls_lf, text="🛡️  Block Updates", command=self.start_block_updates)
        self.btn_block.pack(fill=tk.X, pady=(0, 5))
        
        self.btn_restore = ttk.Button(controls_lf, text="🔓  Restore Original", command=self.start_restore)
        self.btn_restore.pack(fill=tk.X)

        # --- Divider ---
        ttk.Separator(main_content, orient='horizontal').pack(fill=tk.X, pady=(0, 20))

        # --- Download Section ---
        download_frame = ttk.Frame(main_content)
        download_frame.pack(fill=tk.X, pady=(0, 10))
        
        ttk.Label(download_frame, text="Installer Downloader", font=('Segoe UI', 10, 'bold')).pack(anchor=tk.W, pady=(0, 5))
        
        self.version_var = tk.StringVar()
        self.version_dropdown = ttk.Combobox(download_frame, textvariable=self.version_var, values=list(self.versions.keys()), state="readonly")
        self.version_dropdown.pack(fill=tk.X, pady=(0, 8))
        self.version_dropdown.current(1)

        self.btn_download = ttk.Button(download_frame, text="Download Installer", command=self.start_download)
        self.btn_download.pack(fill=tk.X)

        # 3. Log Area (Fixed sizing)
        log_frame = ttk.LabelFrame(root, text="Activity Log", padding="10")
        log_frame.pack(fill=tk.BOTH, expand=True, padx=30, pady=(0, 20))
        
        self.log_area = scrolledtext.ScrolledText(log_frame, height=6, state='disabled', font=self.mono_font, relief=tk.FLAT, bg="#F8F9FA")
        self.log_area.pack(fill=tk.BOTH, expand=True)

        # Initial Logic
        self.refresh_status()

    def log(self, message):
        """Thread-safe logging"""
        self.log_area.config(state='normal')
        self.log_area.insert(tk.END, message + "\n")
        self.log_area.see(tk.END)
        self.log_area.config(state='disabled')

    def refresh_status(self):
        """Update status label based on installation"""
        capcut_path = get_capcut_path()
        if capcut_path.exists():
            self.status_label.config(text=f"✅ CapCut detected at: {capcut_path}", foreground="green")
        else:
            self.status_label.config(text="⚠️ CapCut NOT detected. Please download and install it first.", foreground="#D32F2F")

    def run_threaded(self, target):
        thread = threading.Thread(target=target)
        thread.daemon = True
        thread.start()

    def set_buttons_state(self, state):
        self.btn_block.config(state=state)
        self.btn_download.config(state=state)
        self.btn_restore.config(state=state)
        self.version_dropdown.config(state="readonly" if state == "normal" else "disabled")

    # --- Actions ---

    def start_block_updates(self):
        self.set_buttons_state("disabled")
        self.run_threaded(self.do_block_logic)

    def do_block_logic(self):
        try:
            self.log("-" * 50)
            self.log("🚀 Starting blocking process...")
            
            capcut_path = get_capcut_path()
            if not capcut_path.exists():
                 # Create it if it doesn't exist (user might want to pre-block)
                 self.log(f"   Creating directory: {capcut_path}")
                 capcut_path.mkdir(parents=True, exist_ok=True)

            self.kill_capcut_processes()
            
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
            
            success = self.download_file_bits(download_url, str(installer_path))
            
            if success:
                 self.log("\n✅ Download successfully saved to Downloads folder.")
                 self.log(f"   Path: {installer_path}")
                 
                 # Open explorer to the file
                 subprocess.run(f'explorer /select,"{installer_path}"')
                 
                 messagebox.showinfo("Download Complete", f"Installer saved to your Downloads folder:\n\n{installer_path}\n\nPlease run it manually to install CapCut.")
            else:
                # The browser fallback logic is inside download_file_bits, so if we are here and False, main error happened
                pass

        except Exception as e:
            self.log(f"❌ Error during download: {e}")
        finally:
             self.root.after(0, lambda: self.set_buttons_state("normal"))

    # --- Logic Implementations ---
    
    def kill_capcut_processes(self):
        self.log("🔴 Closing any running CapCut processes...")
        processes = ["CapCut.exe", "CapCutService.exe"]
        for proc in processes:
            try:
                subprocess.run(["taskkill", "/F", "/IM", proc], capture_output=True, check=False)
            except: pass
        time.sleep(1)

    def download_file_bits(self, url, save_path):
        """Try BITS, fallback to Browser"""
        self.log(f"   Target: {Path(save_path).name}")
        self.log("   Method: BITS (Background Transfer)...")
        
        ps_command = f'Start-BitsTransfer -Source "{url}" -Destination "{save_path}" -Priority Foreground'
        try:
            subprocess.run(["powershell", "-NoProfile", "-Command", ps_command], capture_output=True, check=True)
            if os.path.exists(save_path) and os.path.getsize(save_path) > 1000000:
                return True
        except:
            pass
        
        self.log("⚠️ BITS failed (Firewall/Network issue).")
        self.log("   Switching to Browser Fallback...")
        
        try:
            import webbrowser
            self.log("   Opening direct download link in default browser...")
            webbrowser.open(url)
            messagebox.showinfo("Browser Download", "Automated download failed.\n\nWe have opened the direct download link in your browser.\n\nPlease save the file, then install it manually.")
            return False # Return False because we didn't download it ourselves strictly speaking
        except:
            self.log("❌ Failed to open browser.")
            return False

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
        
        for item in version_dirs[1:]:
            try:
                shutil.rmtree(item)
                self.log(f"   Deleted old version: {item.name}")
            except Exception as e:
                 self.log(f"   Error cleaning {item.name}: {e}")

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
            capcut_path = get_capcut_path()
            if not capcut_path.exists():
                self.log("❌ Error: CapCut installation not found.")
                return

            self.kill_capcut_processes()
            
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
        for f in folders:
            fp = userdata_path / f
            if fp.exists():
                try: 
                    shutil.rmtree(fp)
                except: pass

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
