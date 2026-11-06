# Third-party notices

Sastra™ Compressor is © 2026 Ken “Krysion” Nisaka and released under the MIT licence (see `LICENSE`). It includes the following software and fonts, each under its own licence.

## Shipped as a separate program

**FFmpeg** (https://ffmpeg.org), built with **x264** (https://www.videolan.org/developers/x264.html), **Opus** (https://opus-codec.org), and **dav1d** (https://code.videolan.org/videolan/dav1d).
`ffmpeg.exe` is licensed under the GNU General Public License, version 2 or later, because it includes x264. Opus and dav1d are under BSD licences. The full licence texts are in the `licences` folder next to the app, and `BUILD.txt` there gives the exact versions and build settings. The complete source code is attached to every release as `ffmpeg-source-code.zip`, and the build recipe is in the `ffmpeg` folder of this repository.

`ffmpeg.exe` is compiled with the mingw-w64 toolchain (https://www.mingw-w64.org). Parts of its runtime are linked in: the mingw-w64 runtime libraries, under permissive licences, and GCC's runtime libraries, under the GCC Runtime Library Exception.

## Built into the app

Every Rust crate and npm package in the app, with its full licence text, is listed in `THIRD-PARTY-LICENCES.txt`, in the `licences` folder next to the app. To make it from this repository, run `node tools/third-party-licences.mjs`. The main ones:

| Component | Licence | Source |
|-----------|---------|--------|
| Tauri | MIT or Apache 2.0 | https://tauri.app |
| React | MIT | https://react.dev |
| image (Rust crate) | MIT or Apache 2.0 | https://github.com/image-rs/image |
| libwebp, through the webp crate | BSD 3-clause, with an additional patent grant | https://chromium.googlesource.com/webm/libwebp |
| Gabarito font | SIL Open Font Licence 1.1 (`licences/Gabarito-OFL.txt`) | https://github.com/naipefoundry/gabarito |
| Font Awesome Free 7.3.1 | Icons CC BY 4.0, fonts SIL OFL 1.1, code MIT | https://fontawesome.com/license/free |

Font Awesome Free icons by Fonticons, Inc., used under CC BY 4.0.

A few crates that Tauri uses are under the Mozilla Public License 2.0 (`cssparser`, `selectors`, and others; marked in that file). They are unmodified, and their source code is available at the addresses listed there.

The app links Microsoft's WebView2 loader, through the `webview2-com` crates, under Microsoft's WebView2 SDK licence terms (https://www.nuget.org/packages/Microsoft.Web.WebView2/License). It displays its interface with the WebView2 Runtime that is part of Windows, which is not shipped with the app.

The installer is made with NSIS (https://nsis.sourceforge.io), under the zlib/libpng licence with parts under the bzip2 and CPL licences, and Tauri's NSIS tools, under MIT or Apache 2.0.

## Not under the MIT licence

- **The Sastra™ name and logo** are trademarks, and the logo and wordmark files (`src/assets`, `src-tauri/icons`, `src-tauri/installer`) are © Ken “Krysion” Nisaka, all rights reserved. Forks need their own name and logo.
- **The Krysion design language** (`src/design/krysion.css`, `src/design/krysion.js`) is © Ken “Krysion” Nisaka and is included for this app's use. See `src/design/README.md`.
