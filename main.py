"""
Sastra Compressor
=================

A desktop GUI utility for bulk-compressing visual novel assets (images, audio,
and video) into web-friendly formats for the Sastra Visual Novel Engine.

The UI is built with CustomTkinter, while all of the actual media work is handed
off to a bundled `ffmpeg.exe`. FFmpeg is invoked as a subprocess and its progress
output is parsed in real time to drive the progress bar and ETA display.

Threading model:
    Tkinter is single-threaded and not thread-safe. Compression therefore runs
    on a background worker thread, and every UI update from that thread is
    marshalled back onto the main loop via ``self.after(...)``.

Author: Ken "Krysion" Nisaka
License: see LICENSE in the repository root.
"""

import os
import sys
import subprocess
import threading
import time
import re
import ctypes
import customtkinter as ctk
from tkinter import filedialog, messagebox
from PIL import Image
from plyer import notification

# --- Windows taskbar icon fix ---
# By default Windows groups Python scripts under the generic python.exe icon in
# the taskbar. Declaring an explicit AppUserModelID makes Windows treat this app
# as its own entity so our custom icon shows up correctly.
try:
    myappid = 'ken.nisaka.sastracompressor.1'
    ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)
except Exception:
    # Non-Windows platforms (or a missing shell32) simply skip this.
    pass

# --- Theme palette ---
# Dark, high-contrast "neon" look. Tweak these to re-skin the whole app.
BG_COLOR = "#0A0A0E"       # Window background
FRAME_COLOR = "#15151D"    # Card/panel background
ACCENT = "#FF006E"         # Primary action colour (buttons, progress bar)
ACCENT_HOVER = "#CC0058"   # Hover state for accent elements
SUCCESS_GREEN = "#39FF14"  # "Done!" confirmation colour

# --- Supported input formats ---
# Each file's extension decides which FFmpeg pipeline (and settings panel) applies.
IMG_EXTS = ('.jpg', '.jpeg', '.png', '.webp')
VID_EXTS = ('.mp4', '.mkv', '.avi', '.mov')
AUD_EXTS = ('.mp3', '.wav', '.flac', '.ogg')
ALL_EXTS = IMG_EXTS + VID_EXTS + AUD_EXTS


def get_resource_path(relative_path):
    """Resolve a bundled resource path for both source runs and PyInstaller builds.

    When packaged with PyInstaller's ``--onefile`` option, bundled data (ffmpeg,
    icon, logos) is unpacked to a temporary folder exposed as ``sys._MEIPASS``.
    During normal source runs that attribute is absent, so we fall back to the
    directory containing this script. This lets the same code locate assets in
    both contexts.
    """
    if hasattr(sys, '_MEIPASS'):
        return os.path.join(sys._MEIPASS, relative_path)
    return os.path.join(os.path.dirname(__file__), relative_path)

class MediaCompressorApp(ctk.CTk):
    """Main application window and controller for the compressor."""

    def __init__(self):
        super().__init__()

        self.title("Sastra Compressor")
        self.resizable(True, True)
        self.configure(fg_color=BG_COLOR)

        try:
            self.iconbitmap(get_resource_path("icon.ico"))
        except Exception:
            # Missing/locked icon shouldn't stop the app from launching.
            pass

        # --- User-facing settings, bound to the dropdowns/labels in the UI ---
        self.output_dir = ctk.StringVar(value="Same as Source")  # Destination folder, or sentinel
        self.comp_level_img = ctk.StringVar(value="Medium")      # Image: Low/Medium/High
        self.comp_level_vid = ctk.StringVar(value="Medium")      # Video: Low/Medium/High
        self.vid_res = ctk.StringVar(value="Max 1080p")          # Video resolution cap
        self.pic_res = ctk.StringVar(value="Original")           # Image resolution cap
        self.audio_qual = ctk.StringVar(value="Music (96k)")     # Audio bitrate preset

        # --- Runtime state ---
        self.is_processing = False      # True while the worker thread is active
        self.cancel_requested = False   # Set by the Cancel button; polled by the worker
        self.current_process = None     # The live FFmpeg subprocess, so it can be killed
        self.selected_files = []        # Absolute paths queued for compression

        self.setup_ui()

    def setup_ui(self):
        """Build every widget once.

        The window has three swappable "screens" that share one window: the
        initial file/folder picker, the per-media settings panel, and the
        progress view. They are all created here but shown/hidden on demand with
        ``pack``/``pack_forget`` rather than being rebuilt.
        """
        # --- 1. Header (Always Visible) ---
        header_frame = ctk.CTkFrame(self, fg_color="transparent")
        header_frame.pack(pady=(20, 10), padx=40)

        # Logo image. The UI is always dark, so the white logo is used for both
        # CustomTkinter appearance modes. If the asset is missing we fall back to
        # a plain text title so the app still launches.
        try:
            logo_path = get_resource_path(os.path.join("assets", "logo-white.png"))
            logo_img = ctk.CTkImage(light_image=Image.open(logo_path),
                                    dark_image=Image.open(logo_path),
                                    size=(184, 50))
            self.logo_label = ctk.CTkLabel(header_frame, image=logo_img, text="")
            self.logo_label.pack()
        except Exception:
            self.title_label = ctk.CTkLabel(header_frame, text="Media Compressor", font=ctk.CTkFont(size=24, weight="bold"))
            self.title_label.pack()

        self.subtitle = ctk.CTkLabel(header_frame, text="Compress your assets for Sastra visual novels", text_color="gray", font=ctk.CTkFont(size=12))
        self.subtitle.pack(pady=(15, 0)) # Increased top padding for a larger gap

        # --- 2. Initial Selection Buttons ---
        btn_kwargs = {"height": 45, "corner_radius": 22, "fg_color": ACCENT, "hover_color": ACCENT_HOVER, "font": ctk.CTkFont(weight="bold")}
        
        self.selection_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.selection_frame.pack(pady=40, padx=40) # Added horizontal padding

        ctk.CTkButton(self.selection_frame, text="Select File(s)", command=self.select_files, width=160, **btn_kwargs).pack(side="left", padx=10)
        ctk.CTkButton(self.selection_frame, text="Select Folder", command=self.select_folder, width=160, **btn_kwargs).pack(side="right", padx=10)

        # --- 3. Dynamic Container (Hidden Initially) ---
        self.dynamic_container = ctk.CTkFrame(self, fg_color="transparent")
        
        # File List Viewer
        self.file_list_box = ctk.CTkTextbox(self.dynamic_container, height=80, fg_color=FRAME_COLOR, text_color="lightgray", wrap="none")
        
        # Output Path Module
        self.path_frame = ctk.CTkFrame(self.dynamic_container, fg_color="transparent")
        ctk.CTkLabel(self.path_frame, text="Output:").pack(side="left")
        ctk.CTkLabel(self.path_frame, textvariable=self.output_dir, text_color="gray", width=160, anchor="w").pack(side="left", padx=10)
        ctk.CTkButton(self.path_frame, text="Reset", width=50, corner_radius=15, fg_color="#333333", hover_color="#555555", command=lambda: self.output_dir.set("Same as Source")).pack(side="right", padx=(5, 0))
        ctk.CTkButton(self.path_frame, text="Change", width=60, corner_radius=15, fg_color=FRAME_COLOR, hover_color="#2A2A35", command=self.choose_output_dir).pack(side="right")

        dropdown_kwargs = {"width": 140, "fg_color": BG_COLOR, "button_color": ACCENT, "button_hover_color": ACCENT_HOVER, "dropdown_fg_color": FRAME_COLOR}

        # Settings Modules
        self.img_frame = ctk.CTkFrame(self.dynamic_container, fg_color=FRAME_COLOR, corner_radius=10)
        ctk.CTkLabel(self.img_frame, text="Image Settings", font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, columnspan=2, pady=(10, 5))
        ctk.CTkLabel(self.img_frame, text="Compression:").grid(row=1, column=0, sticky="w", padx=15, pady=5)
        ctk.CTkOptionMenu(self.img_frame, variable=self.comp_level_img, values=["Low", "Medium", "High"], **dropdown_kwargs).grid(row=1, column=1, sticky="e", padx=15, pady=5)
        ctk.CTkLabel(self.img_frame, text="Resolution:").grid(row=2, column=0, sticky="w", padx=15, pady=(5, 15))
        ctk.CTkOptionMenu(self.img_frame, variable=self.pic_res, values=["Original", "Max 1080p", "Max 720p"], **dropdown_kwargs).grid(row=2, column=1, sticky="e", padx=15, pady=(5, 15))

        self.vid_frame = ctk.CTkFrame(self.dynamic_container, fg_color=FRAME_COLOR, corner_radius=10)
        ctk.CTkLabel(self.vid_frame, text="Video Settings", font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, columnspan=2, pady=(10, 5))
        ctk.CTkLabel(self.vid_frame, text="Compression:").grid(row=1, column=0, sticky="w", padx=15, pady=5)
        ctk.CTkOptionMenu(self.vid_frame, variable=self.comp_level_vid, values=["Low", "Medium", "High"], **dropdown_kwargs).grid(row=1, column=1, sticky="e", padx=15, pady=5)
        ctk.CTkLabel(self.vid_frame, text="Resolution:").grid(row=2, column=0, sticky="w", padx=15, pady=(5, 15))
        ctk.CTkOptionMenu(self.vid_frame, variable=self.vid_res, values=["Original", "Max 1080p", "Max 720p"], **dropdown_kwargs).grid(row=2, column=1, sticky="e", padx=15, pady=(5, 15))

        self.aud_frame = ctk.CTkFrame(self.dynamic_container, fg_color=FRAME_COLOR, corner_radius=10)
        ctk.CTkLabel(self.aud_frame, text="Audio Settings", font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, columnspan=2, pady=(10, 5))
        ctk.CTkLabel(self.aud_frame, text="Quality:").grid(row=1, column=0, sticky="w", padx=15, pady=(5, 15))
        ctk.CTkOptionMenu(self.aud_frame, variable=self.audio_qual, values=["Music (96k)", "SFX & Voice (64k)"], **dropdown_kwargs).grid(row=1, column=1, sticky="e", padx=15, pady=(5, 15))

        # Action Buttons
        self.action_frame = ctk.CTkFrame(self.dynamic_container, fg_color="transparent")
        self.btn_compress = ctk.CTkButton(self.action_frame, text="Compress", command=self.start_processing, width=160, **btn_kwargs)
        self.btn_compress.pack(side="left", padx=10)
        ctk.CTkButton(self.action_frame, text="Clear Selection", command=self.clear_selection, width=100, height=45, corner_radius=22, fg_color="#333333", hover_color="#555555", font=ctk.CTkFont(weight="bold")).pack(side="right", padx=10)

        # --- 4. Progress Elements (Hidden Initially) ---
        self.progress_frame = ctk.CTkFrame(self, fg_color="transparent")
        self.status_label = ctk.CTkLabel(self.progress_frame, text="", text_color="yellow")
        self.status_label.pack(pady=(0, 5))
        self.progress_bar = ctk.CTkProgressBar(self.progress_frame, mode="determinate", progress_color=ACCENT)
        self.progress_bar.set(0)
        self.progress_bar.pack(fill="x", padx=30)
        self.eta_label = ctk.CTkLabel(self.progress_frame, text="", text_color="gray", font=ctk.CTkFont(size=11))
        self.eta_label.pack()
        self.btn_cancel = ctk.CTkButton(self.progress_frame, text="Cancel", width=100, height=30, corner_radius=20, fg_color="#333333", hover_color="#FF4444", command=self.cancel_processing)
        
        # --- 5. Footer (Always Visible) ---
        footer_frame = ctk.CTkFrame(self, fg_color="transparent")
        footer_frame.pack(side="bottom", fill="x", pady=10)
        ctk.CTkLabel(footer_frame, text="© 2026 Ken “Krysion” Nisaka  •  Powered by FFmpeg", text_color="#555566", font=ctk.CTkFont(size=10)).pack()

    # ------------------------------------------------------------------ #
    #  UI interactions & screen flow                                      #
    # ------------------------------------------------------------------ #
    def choose_output_dir(self):
        """Prompt for a custom output folder (otherwise files save next to source)."""
        directory = filedialog.askdirectory(title="Select Output Folder")
        if directory: self.output_dir.set(directory)

    def clear_selection(self):
        """Return to the initial picker screen and drop the current queue."""
        self.selected_files = []
        self.dynamic_container.pack_forget()
        self.file_list_box.pack_forget()
        self.img_frame.pack_forget()
        self.vid_frame.pack_forget()
        self.aud_frame.pack_forget()
        self.selection_frame.pack(pady=40, padx=40)

    def select_files(self):
        """Pick one or more individual media files via a file dialog."""
        files = filedialog.askopenfilenames(title="Select Media Files", filetypes=[("Media Files", " ".join(f"*{ext}" for ext in ALL_EXTS))])
        if files: self.build_dynamic_ui(files)

    def select_folder(self):
        """Pick a folder and queue every supported (non-recursive) media file in it."""
        directory = filedialog.askdirectory(title="Select Folder to Compress")
        if directory:
            files = [os.path.join(directory, f) for f in os.listdir(directory) if f.lower().endswith(ALL_EXTS)]
            if not files:
                messagebox.showinfo("No Media", "No supported media files found in this folder.")
                return
            self.build_dynamic_ui(files)

    def build_dynamic_ui(self, files):
        """Swap to the settings screen, showing only the panels relevant to the queue.

        e.g. an audio-only selection won't show the image or video settings.
        """
        self.selected_files = files
        self.selection_frame.pack_forget()

        # Generous horizontal padding here keeps it from sticking to the edges
        self.dynamic_container.pack(fill="x", padx=50, pady=10)

        # Populate file list
        self.file_list_box.pack(fill="x", pady=(0, 15))
        self.file_list_box.configure(state="normal")
        self.file_list_box.delete("1.0", "end")
        for f in files:
            self.file_list_box.insert("end", f"• {os.path.basename(f)}\n")
        self.file_list_box.configure(state="disabled")

        self.path_frame.pack(fill="x", pady=(0, 15))

        # Only reveal the settings panels for media types actually present.
        has_img = any(f.lower().endswith(IMG_EXTS) for f in files)
        has_vid = any(f.lower().endswith(VID_EXTS) for f in files)
        has_aud = any(f.lower().endswith(AUD_EXTS) for f in files)

        if has_img:
            self.img_frame.pack(fill="x", pady=5)
            self.img_frame.grid_columnconfigure((0, 1), weight=1)
        if has_vid:
            self.vid_frame.pack(fill="x", pady=5)
            self.vid_frame.grid_columnconfigure((0, 1), weight=1)
        if has_aud:
            self.aud_frame.pack(fill="x", pady=5)
            self.aud_frame.grid_columnconfigure((0, 1), weight=1)

        self.action_frame.pack(pady=15)

    def cancel_processing(self):
        """Request cancellation and kill the in-flight FFmpeg process, if any.

        The worker loop checks ``cancel_requested`` between files; killing the
        current process unblocks the stderr read so cancellation is near-instant.
        """
        self.cancel_requested = True
        self.status_label.configure(text="Cancelling...", text_color="red")
        if self.current_process:
            try: self.current_process.kill()
            except Exception: pass

    # ------------------------------------------------------------------ #
    #  Compression pipeline (runs off the main/UI thread)                 #
    # ------------------------------------------------------------------ #
    def start_processing(self):
        """Switch to the progress screen and kick off the worker thread."""
        self.is_processing = True
        self.cancel_requested = False
        self.dynamic_container.pack_forget()
        self.progress_frame.pack(fill="x", padx=40, pady=20)
        self.btn_cancel.pack(pady=10)
        # daemon=True so the thread won't keep the process alive if the window closes.
        threading.Thread(target=self.process_queue, args=(self.selected_files,), daemon=True).start()

    def process_queue(self, file_list):
        """Worker entry point: build and run an FFmpeg command for each queued file.

        Runs on a background thread. Per file, the extension selects one of three
        pipelines and the user's settings are translated into FFmpeg flags:

          * Images -> WebP (libwebp), quality by compression level.
          * Video  -> MP4/H.264 (yuv420p, +faststart) for instant browser streaming.
          * Audio  -> OGG/Opus (libopus, VBR) at the chosen bitrate.

        Each output is written next to the source as ``<name>_compressed.<ext>`` so
        originals are never overwritten.
        """
        total_files = len(file_list)
        ffmpeg_path = get_resource_path('ffmpeg.exe')
        out_dir = self.output_dir.get()

        for index, input_path in enumerate(file_list):
            if self.cancel_requested: break

            filename = os.path.basename(input_path)
            self.update_ui_status(f"Processing {index + 1}/{total_files}: {filename}", 0, "Calculating ETA...")

            base_name, ext = os.path.splitext(filename)
            ext = ext.lower()
            save_dir = os.path.dirname(input_path) if out_dir == "Same as Source" else out_dir
            
            c_img = self.comp_level_img.get()
            c_vid = self.comp_level_vid.get()
            v_res = self.vid_res.get()
            p_res = self.pic_res.get()
            a_qual = "64k" if "64k" in self.audio_qual.get() else "96k"

            # "-y" overwrites any stale output from a previous run without prompting.
            cmd = [ffmpeg_path, "-y", "-i", input_path]

            if ext in IMG_EXTS:
                # Higher -q:v = better quality/larger file, so Low compression keeps it high.
                output_path = os.path.join(save_dir, f"{base_name}_compressed.webp")
                q_val = "90" if c_img == "Low" else "75" if c_img == "Medium" else "50"
                cmd.extend(["-c:v", "libwebp", "-q:v", q_val])
                # scale='min(W,iw)':-2 downscales only if wider than the cap; -2 keeps the
                # aspect ratio while forcing an even height (required by many codecs).
                if p_res == "Max 1080p": cmd.extend(["-vf", "scale='min(1920,iw)':-2"])
                elif p_res == "Max 720p": cmd.extend(["-vf", "scale='min(1280,iw)':-2"])

            elif ext in VID_EXTS:
                # Higher CRF = more compression/lower quality; slower preset = smaller file.
                output_path = os.path.join(save_dir, f"{base_name}_compressed.mp4")
                crf = "22" if c_vid == "Low" else "26" if c_vid == "Medium" else "30"
                preset = "slow" if c_vid == "Low" else "slower" if c_vid == "Medium" else "veryslow"
                cmd.extend([
                    "-c:v", "libx264", "-preset", preset, "-crf", crf,
                    "-profile:v", "high", "-level", "4.1", "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart", "-c:a", "aac", "-b:a", a_qual
                ])
                if v_res == "Max 1080p": cmd.extend(["-vf", "scale='min(1920,iw)':-2"])
                elif v_res == "Max 720p": cmd.extend(["-vf", "scale='min(1280,iw)':-2"])

            elif ext in AUD_EXTS:
                output_path = os.path.join(save_dir, f"{base_name}_compressed.ogg")
                cmd.extend(["-c:a", "libopus", "-b:a", a_qual, "-vbr", "on"])

            # "-progress pipe:2" emits machine-readable progress on stderr; "-nostats"
            # suppresses the noisy human-readable stats that would otherwise interleave.
            cmd.extend(["-progress", "pipe:2", "-nostats", output_path])

            try:
                self.run_ffmpeg_with_progress(cmd, input_path, ffmpeg_path)
            except Exception:
                # Skip a file that fails (e.g. corrupt input) and keep the queue going.
                pass

        # Hop back to the UI thread to finalise.
        self.after(0, self.finish_processing)

    def get_media_duration(self, ffmpeg_path, input_path):
        """Return the media's duration in seconds by parsing FFmpeg's probe output.

        Used to convert FFmpeg's elapsed time into a 0-1 progress fraction. Falls
        back to a tiny non-zero value so progress math never divides by zero.
        """
        cmd = [ffmpeg_path, "-i", input_path]
        result = subprocess.run(cmd, stderr=subprocess.PIPE, text=True, creationflags=subprocess.CREATE_NO_WINDOW)
        match = re.search(r"Duration: (\d{2}):(\d{2}):(\d{2}\.\d{2})", result.stderr)
        if match:
            h, m, s = match.groups()
            return int(h) * 3600 + int(m) * 60 + float(s)
        return 0.1

    def run_ffmpeg_with_progress(self, cmd, input_path, ffmpeg_path):
        """Run one FFmpeg job, streaming live progress + ETA back to the UI.

        Reads the ``-progress`` key/value lines as they arrive and derives a
        percentage and ETA from FFmpeg's reported output timestamp.
        ``CREATE_NO_WINDOW`` stops a console window flashing on each invocation.
        """
        duration = self.get_media_duration(ffmpeg_path, input_path)
        self.current_process = subprocess.Popen(cmd, stderr=subprocess.PIPE, text=True, creationflags=subprocess.CREATE_NO_WINDOW)

        start_time = time.time()
        for line in self.current_process.stderr:
            if self.cancel_requested: break
            # out_time_us = microseconds of media processed so far.
            if "out_time_us=" in line:
                try:
                    cur_sec = int(line.split("=")[1].strip()) / 1_000_000
                    progress = min(cur_sec / duration, 1.0)
                    # ETA = remaining media / current processing speed (media-sec per wall-sec).
                    elapsed = time.time() - start_time
                    speed = cur_sec / elapsed if elapsed > 0 else 0
                    eta_sec = (duration - cur_sec) / speed if speed > 0 else 0
                    eta_str = time.strftime('%H:%M:%S', time.gmtime(eta_sec))
                    self.after(0, self._apply_progress, progress, f"{int(progress*100)}% - ETA: {eta_str}")
                except ValueError: pass

        self.current_process.wait()

    # --- Thread-safe UI bridges -------------------------------------- #
    # These are called from the worker thread via self.after(...), which queues
    # them to run on the main loop. The paired _apply_* helpers do the actual
    # widget updates and must only ever run on the main thread.
    def update_ui_status(self, text, prog_val, eta_text):
        """Queue a full status update (label + bar + ETA) onto the UI thread."""
        self.after(0, self._apply_status, text, prog_val, eta_text)

    def _apply_status(self, text, prog_val, eta_text):
        self.status_label.configure(text=text)
        self.progress_bar.set(prog_val)
        self.eta_label.configure(text=eta_text)

    def _apply_progress(self, prog_val, eta_text):
        self.progress_bar.set(prog_val)
        self.eta_label.configure(text=eta_text)

    def send_notification(self):
        """Fire a desktop "all done" toast (best-effort; ignored if unsupported)."""
        try:
            icon_path = get_resource_path("icon.ico")
            if not os.path.exists(icon_path): icon_path = None
            
            notification.notify(
                title="Sastra Compressor",
                message="All asset compressions complete!",
                app_name="Sastra Compressor",
                app_icon=icon_path,
                timeout=5
            )
        except Exception as e:
            print(f"Notification failed: {e}")

    def finish_processing(self):
        """Wrap up the queue: show the result, notify, then auto-reset the UI."""
        self.is_processing = False
        self.current_process = None

        # Hide the cancel button immediately
        self.btn_cancel.pack_forget()

        if self.cancel_requested:
            self.status_label.configure(text="Cancelled.", text_color="red")
        else:
            self.status_label.configure(text="Tasks complete!", text_color=SUCCESS_GREEN)
            self.progress_bar.set(1.0)
            threading.Thread(target=self.send_notification, daemon=True).start()
            
        self.eta_label.configure(text="")
        self.after(2500, self.reset_progress_ui)

    def reset_progress_ui(self):
        """Tear down the progress view and return to the picker (unless a new run started)."""
        if not self.is_processing:
            self.progress_frame.pack_forget()
            self.clear_selection()


if __name__ == "__main__":
    app = MediaCompressorApp()
    app.mainloop()