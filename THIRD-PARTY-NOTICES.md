# Third-party notices

Sastra™ Compressor is © 2026 Ken “Krysion” Nisaka and released under the MIT licence (see `LICENSE`). It includes the following software and fonts, each under its own licence.

## Shipped as a separate program

**FFmpeg** (https://ffmpeg.org), built with **x264** (https://www.videolan.org/developers/x264.html), **Opus** (https://opus-codec.org), and **dav1d** (https://code.videolan.org/videolan/dav1d).
`ffmpeg.exe` is licensed under the GNU General Public License, version 2 or later, because it includes x264. Opus and dav1d are under BSD licences. The full licence texts are in the `licences` folder next to the app, and `BUILD.txt` there gives the exact versions and build settings. The complete source code is attached to every release as `ffmpeg-source-code.zip`, and the build recipe is in the `ffmpeg` folder of this repository.

## Built into the app

| Component | Licence | Source |
|-----------|---------|--------|
| Tauri | MIT or Apache 2.0 | https://tauri.app |
| React | MIT | https://react.dev |
| image (Rust crate) | MIT or Apache 2.0 | https://github.com/image-rs/image |
| libwebp, through the webp crate | BSD 3-clause | https://chromium.googlesource.com/webm/libwebp |
| Gabarito font | SIL Open Font Licence 1.1 (`licences/Gabarito-OFL.txt`) | https://github.com/naipefoundry/gabarito |
| Font Awesome Free 7.3.1 | Icons CC BY 4.0, fonts SIL OFL 1.1, code MIT | https://fontawesome.com/license/free |

Font Awesome Free icons by Fonticons, Inc., used under CC BY 4.0. The other Rust and JavaScript libraries the app is built from are listed, with their versions, in `src-tauri/Cargo.lock` and `package-lock.json`; each keeps its own licence.

The Sastra™ name and logo are trademarks and are not covered by the MIT licence.
