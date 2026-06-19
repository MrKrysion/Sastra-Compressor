<div align="center">
  <a href="https://www.sastra.dev">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="assets/logo-white.png">
      <source media="(prefers-color-scheme: light)" srcset="assets/logo-black.png">
      <img alt="Sastra Logo" src="assets/logo-black.png" width="350">
    </picture>
  </a>
  <h1>Sastra Compressor</h1>
  <p><strong>A media compression utility for use alongside the Sastra Visual Novel Engine.</strong></p>
  <p>
    <sub>Sastra links:</sub>
    <br/>
    <a href="https://www.sastra.dev">Sastra Website</a> •
    <a href="https://www.sastra.dev/docs">Sastra Documentation</a> •
    <a href="https://www.sastra.dev/extras">Sastra Extras</a>
  </p>
</div>

## 📦 Overview

**Sastra Compressor** is a desktop utility built to take the headache out of web-asset optimisation.

Because the [Sastra Visual Novel Engine](https://www.sastra.dev) runs in the browser via HTML/JS/CSS, serving massive raw images and uncompressed audio can ruin the player experience with long loading screens. This tool provides a beautiful, user-friendly GUI to bulk-compress your visual novel assets (Images, Audio, and Video) into highly optimised, web-friendly formats using FFmpeg.

### Key Features

* **🖼️ Image Compression:** Converts `.jpg`, `.png`, and other formats to highly optimised `.webp` files. Includes optional auto-scaling (Max 1080p or 720p).
* **🎵 Audio Compression:** Converts audio to `.ogg` (libopus) optimised for Music (96k) or SFX/Voice (64k).
* **🎥 Video Compression:** Converts videos into web-standard `.mp4` (H.264, yuv420p) with `-movflags +faststart` for instant browser streaming.
* **📊 Live Progress:** Parses FFmpeg streams in real-time to provide accurate progress bars and ETAs.

## 🛠️ Prerequisites & Installation

If you prefer to run the compressor from the source code rather than using a pre-compiled binary, you will need to set up your environment.

1. **Install Python 3.8+**

2. **Install Dependencies:**

   ```bash
   pip install customtkinter Pillow plyer
   ```

3. **Get FFmpeg:** This app requires the `ffmpeg` executable to function. [Download the latest release of FFmpeg](https://www.ffmpeg.org/download.html#build-windows), and place the `ffmpeg.exe` directly into the root directory of this project.

4. **Run the App:**

   ```bash
   python main.py
   ```

## 🚀 Usage Guide

1. **Select Assets:** Click **Select File(s)** to pick individual media files, or **Select Folder** to grab everything inside a directory.

2. **Set Output:** Choose where you want the compressed files saved. By default, it saves them alongside the source files.

3. **Tweak Settings:** Adjust compression levels (Low, Medium, High) and resolution limits based on your VN's needs.

4. **Compress:** Hit the Compress button. The app will process your queue and send a desktop notification when all assets are ready to be dropped into your Sastra asset folders!

## 📦 Building from Source (PyInstaller)

If you want to package this script into a standalone `.exe` to share with your team (or distribute), you can use PyInstaller. The code is already configured to unpack bundled assets (the `assets/` logo folder, `icon.ico`, and `ffmpeg.exe`) via `sys._MEIPASS`.

```bash
pip install pyinstaller
```
```bash
pyinstaller --noconsole --onefile --icon=icon.ico --hidden-import plyer.platforms.win.notification --add-binary "ffmpeg.exe;." --add-data "icon.ico;." --add-data "assets;assets" main.py
```

> _Note: Bundling `ffmpeg.exe` inside a `--onefile` build will make the resulting `.exe` quite large and may increase the startup time slightly as it extracts the binary to a temp folder._