# ffmpeg for Sastra™ Compressor

Sastra Compressor uses its own build of [FFmpeg](https://ffmpeg.org) for audio and video. It contains only what the app needs, so it is about 9 MB instead of the 80–140 MB of a general-purpose build. Images don't use ffmpeg: the app compresses them itself.

| File | What it does |
|------|--------------|
| `versions.sh` | The pinned version and checksum of every source. |
| `build.sh` | Downloads the sources and cross-compiles a static `ffmpeg.exe` for 64-bit Windows from Linux. |
| `test.sh` | Makes a sample of every accepted format and checks that `ffmpeg.exe` converts each one. |

GitHub Actions runs both scripts (`.github/workflows/ffmpeg.yml`) whenever this folder changes, and the app build picks up the result. You never need to build ffmpeg yourself.

## What goes in

| | Accepted |
|---|---|
| Video files | MP4, M4V, MOV, MKV, WebM, and AVI |
| Video codecs | H.264, H.265 (HEVC), AV1, VP9, VP8, ProRes, DNxHD/DNxHR, MPEG-4 Part 2, Motion JPEG, and uncompressed |
| Audio files | WAV, W64, FLAC, MP3, OGG, OGA, Opus, M4A, AAC, AIFF, and AIF |
| Audio codecs | PCM (8-, 16-, 24-, and 32-bit, and float), FLAC, MP3, AAC, ALAC, Vorbis, Opus, AC-3, and E-AC-3 |

What comes out: H.264 video with AAC audio in MP4 (x264), and Opus audio in Ogg (libopus).

To accept a new format, add its demuxer, decoder, and parser in `build.sh`, its extension in `src-tauri/src/formats.rs`, and a sample in `test.sh`. Then bump `SASTRA_BUILD` in `versions.sh` if nothing else changed there.

## Building it yourself

On Ubuntu 24.04 or Debian (WSL works):

```bash
sudo apt-get install mingw-w64 nasm meson ninja-build pkg-config curl xz-utils git
```
```bash
ffmpeg/build.sh
```

The result is in `ffmpeg/work/out`, with `BUILD.txt` (versions and the full configure line), `licences/`, and `sources/` (the exact source code used).

## Licences

This build includes x264, so it is licensed under the **GNU GPL** (version 2 or later). Its source code, licences, and build recipe ship with every release of the app. The app itself stays MIT-licensed: it runs ffmpeg as a separate program. Opus and dav1d are under BSD licences.
