<div align="center">
  <a href="https://www.sastra.dev">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="src/assets/wordmark-white.svg">
      <source media="(prefers-color-scheme: light)" srcset="src/assets/wordmark-black.svg">
      <img alt="Sastra™" src="src/assets/wordmark-black.svg" width="350">
    </picture>
  </a>
  <h1>Sastra™ Compressor</h1>
  <p><strong>A media compression app for use alongside the Sastra™ visual novel engine.</strong></p>
  <p>
    <sub>Sastra™ links:</sub>
    <br/>
    <a href="https://github.com/MrKrysion/Sastra">Sastra™ Engine (GitHub)</a> •
    <a href="https://www.sastra.dev">Sastra™ website</a> •
    <a href="https://www.sastra.dev/docs">Sastra™ documentation</a> •
    <a href="https://www.sastra.dev/extras">Sastra™ extras</a>
  </p>
</div>

## 📦 Overview

**Sastra™ Compressor** takes the headache out of preparing assets for the web.

The [Sastra™ visual novel engine](https://github.com/MrKrysion/Sastra) runs in the browser, so large raw images and uncompressed audio mean long loading screens for players. Drop your images, audio, and video into Sastra™ Compressor and it turns them into small, web-friendly files, ready to go into your project's asset folders.

### What it does

* **🖼️ Images → WebP.** PNG, JPEG, WebP, BMP, TIFF, and GIF. Transparency and colour profiles are kept, animated images stay animated, photos are turned upright, and wide images can be shrunk to 1920 or 1280 px.
* **🎵 Audio → Opus in Ogg.** WAV, FLAC, MP3, OGG, Opus, M4A, AAC, AIFF, and W64, at 96 kbit/s for music or 64 kbit/s for voice and sound effects.
* **🎥 Video → H.264 MP4.** MP4, MOV, MKV, WebM, and AVI, with `+faststart` so browsers can start playing straight away, and an optional maximum frame rate.
* **📁 Whole folders.** Drop a folder and everything inside is added, subfolders included.
* **💾 Three ways to save.** Next to the originals (as `name_compressed`), into another folder with the same names and subfolders, or in place of the originals (which go to the Recycle Bin, after asking first; on drives with no Bin, such as USB drives, the app warns that they are deleted for good).
* **📊 Live progress.** Each file shows its progress and how much smaller it became; the taskbar shows overall progress, and a Windows notification tells you when everything is done.

## ⬇️ Download

Get the latest version from [Releases](https://github.com/MrKrysion/Sastra-Compressor/releases): the **setup** file installs it, or the **portable** zip runs from any folder. Windows 10 and 11, 64-bit.

## 🛠️ Working on it

Sastra™ Compressor is a [Tauri](https://tauri.app) app: the interface is React (in `src/`), the compressing is Rust (in `src-tauri/`), and audio and video go through a small build of FFmpeg made just for it (in `ffmpeg/`).

### Preview the interface in a browser

Needs [Node.js](https://nodejs.org) 22 or later. A stand-in fakes the files and the compressing, so you can work on the design without building anything else.

```bash
npm install
```
```bash
npm run dev
```

Then open http://localhost:1420.

### Run the real app on Windows

Also needs [Rust](https://rustup.rs) and the Visual Studio Build Tools with the "Desktop development with C++" workload ([Tauri's guide](https://tauri.app/start/prerequisites/)).

1. Download `ffmpeg.exe`: open the latest run of the **app** workflow under [Actions](https://github.com/MrKrysion/Sastra-Compressor/actions), and download the `ffmpeg-windows` artifact. Or take it from a portable zip.
2. Save it as `src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe`.
3. Run:

```bash
npm run tauri dev
```

To make an installer locally, run `npm run tauri build`.

### The design language

The app uses the Krysion design language with the Sastra™ palette. `krysion.css`, `krysion.js`, and the Gabarito font live in `src/design/`, so the app works offline and depends on nothing outside this repository. To update the design language, copy the new versions in by hand; `src/design/README.md` lists the two lines of `krysion.css` that change when they're copied.

The app's own styles are in `src/app.css`. The window uses the standard Windows title bar for now; the header inside the window is `src/components/Header.tsx`.

### Releasing

1. Update `version` in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml`.
2. Publish a release on GitHub with a new tag (for example `v2.1.0`).
3. GitHub Actions builds the app and attaches the installer, the portable zip, and FFmpeg's source code to the release.

## 📄 Licence

Sastra™ Compressor is released under the [MIT licence](LICENSE). The Sastra™ name and logo and the Krysion design language files aren't covered by it. The bundled FFmpeg is under the GNU GPL. See [third-party notices](THIRD-PARTY-NOTICES.md) and [ffmpeg/README.md](ffmpeg/README.md).
