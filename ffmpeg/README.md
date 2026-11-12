# ffmpeg for Sastra™ Compressor

Sastra Compressor uses its own build of [FFmpeg](https://ffmpeg.org) for audio and video. It contains only what the app needs, so it is about 9 MB instead of the 80–140 MB of a general-purpose build. Images don't use ffmpeg: the app compresses them itself.

| File | What it does |
|------|--------------|
| `versions.sh` | The pinned version and checksum of every source. |
| `build.sh` | Downloads the sources and cross-compiles a static `ffmpeg.exe` for 64-bit Windows from Linux. |
| `test.sh` | Makes a sample of every accepted format, checks that `ffmpeg.exe` converts each one, and checks that no patented codec is built in. |

GitHub Actions runs both scripts (`.github/workflows/ffmpeg.yml`) as part of the app build, and only when the recipe has changed; otherwise it reuses the last result. You never need to build ffmpeg yourself.

## What goes in

| | Accepted |
|---|---|
| Video files | MP4, M4V, MOV, MKV, WebM, and AVI |
| Video codecs | H.264 and H.265 (HEVC), read by Windows; AV1, VP9, VP8, MPEG-4 Part 2, Motion JPEG, QuickTime Animation, HuffYUV, Ut Video, FFV1, MPEG-1, MPEG-2, DivX 3, WMV 8, and uncompressed |
| Audio files | WAV, W64, FLAC, MP3, OGG, OGA, Opus, M4A, AAC, AIFF, and AIF |
| Audio codecs | AAC and E-AC-3, read by Windows; PCM (8-, 16-, 24-, and 32-bit, float, A-law, and mu-law), ADPCM, FLAC, MP3, MP2, ALAC, Vorbis, Opus, AC-3, and DTS |

What comes out: VP9 video with Opus audio in WebM (libvpx, libopus), and Opus audio in Ogg (libopus).

**No patented codecs are built in.** H.264, HEVC, AAC, and E-AC-3 are still accepted, but Windows decodes them (Media Foundation, whose patent licences Microsoft holds) and the app pipes the raw frames and sound into ffmpeg. ProRes and DNxHD aren't accepted: re-export those as H.264. `test.sh` fails if one of those decoders or encoders creeps back in.

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

This build is licensed under the **GNU LGPL** (version 2.1 or later). Its source code, licences, and build recipe ship with every release of the app. The app itself stays MIT-licensed, and runs ffmpeg as a separate program. libvpx, Opus, and dav1d are under BSD licences.
