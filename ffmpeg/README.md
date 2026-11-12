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
| Video files | MP4, M4V, MOV, MKV, WebM, AVI, WMV, ASF, MPG, MPEG, M2V, VOB, TS, M2TS, MTS, 3GP, 3G2, and OGV |
| Video codecs | H.264, HEVC, and VC-1 (WMV 9), read by Windows; AV1, VP9, VP8, Theora, MPEG-1, MPEG-2, MPEG-4 Part 2, MS-MPEG4 (DivX 3 and earlier), WMV 7 and 8, DV, H.263, Cinepak, Microsoft Video 1, Indeo 2 to 5, Sorenson Video 1, Motion JPEG, QuickTime Animation, HuffYUV, Ut Video, FFV1, and uncompressed |
| Audio files | WAV, W64, FLAC, MP3, MP2, MPA, OGG, OGA, Opus, SPX, M4A, M4B, AAC, AIFF, AIF, AIFC, WMA, WV, APE, TTA, MPC, AMR, 3GA, AC3, EAC3, DTS, CAF, MKA, WEBA, AU, and SND |
| Audio codecs | AAC, WMA Pro, Lossless, and Voice, and AMR, read by Windows; PCM (8- to 64-bit, float, A-law, and mu-law), ADPCM (Microsoft, IMA, QuickTime IMA4), FLAC, MP3, MP2, ALAC, Vorbis, Opus, Speex, AC-3, E-AC-3, DTS, WMA 1 and 2, WavPack, Monkey's Audio, TTA, and Musepack |
| Pictures | AVIF (the app reads it through ffmpeg; everything else is read by the app itself) |

What comes out: VP9 video with Opus audio in WebM (libvpx, libopus), Opus audio in Ogg (libopus), and raw RGBA for an AVIF picture, which the app turns into WebP.

**No patented codecs are built in.** H.264, HEVC, VC-1, AAC, WMA Pro, Lossless, and Voice, and AMR are still accepted, but Windows decodes them (Media Foundation, whose patent licences Microsoft holds) and the app pipes the raw frames and sound into ffmpeg. ProRes and DNxHD aren't accepted: re-export those as H.264. `test.sh` fails if one of those decoders or encoders creeps back in.

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
