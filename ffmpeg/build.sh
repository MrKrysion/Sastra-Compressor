#!/usr/bin/env bash
# Cross-compiles a small, static ffmpeg.exe for 64-bit Windows from Linux
# (Ubuntu or Debian), with only what Sastra Compressor needs.
#
#   sudo apt-get install mingw-w64 nasm meson ninja-build pkg-config curl xz-utils git
#   ./build.sh [work-dir]
#
# Output, in <work-dir>/out:
#   ffmpeg.exe            the program
#   licences/             licence texts of ffmpeg and every library in it
#   sources/              the exact source code used (LGPL: shipped with releases)
#   BUILD.txt             versions and the full configure line
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=versions.sh
source "$HERE/versions.sh"

WORK="$(realpath -m "${1:-$HERE/work}")"
SRC="$WORK/src"; PREFIX="$WORK/prefix"; OUT="$WORK/out"
HOST=x86_64-w64-mingw32
JOBS="$(nproc)"
mkdir -p "$SRC" "$PREFIX" "$OUT/licences" "$OUT/sources"

export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export CFLAGS="-O2"

fetch() { # url sha256 file
  local url=$1 sum=$2 file="$SRC/$3"
  [ -f "$file" ] || curl -fsSL --retry 4 -o "$file" "$url"
  echo "$sum  $file" | sha256sum -c --quiet
}

unpack() { # file dir
  rm -rf "$SRC/$2"; mkdir -p "$SRC/$2"
  tar -xf "$SRC/$1" -C "$SRC/$2" --strip-components=1
}

echo "== libvpx"
fetch "$LIBVPX_URL" "$LIBVPX_SHA256" libvpx.tar.gz
unpack libvpx.tar.gz libvpx
# Only the VP9 encoder: ffmpeg's own vp8 and vp9 decoders read WebM.
( cd "$SRC/libvpx"
  CROSS=$HOST- ./configure --target=x86_64-win64-gcc --prefix="$PREFIX" --as=nasm     --enable-static --disable-shared --disable-examples --disable-tools --disable-docs     --disable-unit-tests --disable-install-bins --disable-install-srcs     --disable-vp8 --disable-vp9-decoder
  make -j"$JOBS" && make install )

echo "== opus"
fetch "$OPUS_URL" "$OPUS_SHA256" opus.tar.gz
unpack opus.tar.gz opus
( cd "$SRC/opus"
  ./configure --host=$HOST --prefix="$PREFIX" --enable-static --disable-shared \
    --disable-doc --disable-extra-programs
  make -j"$JOBS" && make install )

echo "== dav1d"
fetch "$DAV1D_URL" "$DAV1D_SHA256" dav1d.tar.xz
unpack dav1d.tar.xz dav1d
cat > "$WORK/cross.meson" <<EOF
[binaries]
c = '$HOST-gcc'
ar = '$HOST-ar'
strip = '$HOST-strip'
windres = '$HOST-windres'
[host_machine]
system = 'windows'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
EOF
( cd "$SRC/dav1d"
  meson setup build --cross-file "$WORK/cross.meson" --prefix="$PREFIX" --libdir=lib \
    --buildtype=release --default-library=static -Denable_tools=false -Denable_tests=false
  ninja -C build && ninja -C build install )

echo "== ffmpeg"
fetch "$FFMPEG_URL" "$FFMPEG_SHA256" ffmpeg.tar.xz
unpack ffmpeg.tar.xz ffmpeg

# Inputs we accept, and only the parts of ffmpeg needed to read them.
# Keep this list in step with src-tauri/src/formats.rs in the app.
# No patented codecs: H.264, HEVC, AAC, and E-AC-3 are read by the operating
# system, which pipes raw frames ("rawvideo", NV12) and PCM sound in. ProRes and
# DNxHD aren't read at all. Containers are still demuxed here: reading headers isn't decoding.
DEMUXERS=mov,matroska,avi,mp3,wav,w64,flac,ogg,aac,aiff,rawvideo
# Besides everyday codecs: what editors export (QuickTime Animation, HuffYUV,
# Ut Video, FFV1), and older video and sound, since one unreadable track fails a whole video.
VIDEO_DECODERS=mpeg4,mjpeg,vp8,vp9,libdav1d,rawvideo,qtrle,huffyuv,ffvhuff,utvideo,ffv1,mpeg2video,mpeg1video,msmpeg4v3,wmv2
AUDIO_DECODERS=mp3float,flac,vorbis,opus,alac,ac3,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,pcm_s16be,pcm_s24be,pcm_s32be,pcm_u8,pcm_s8,pcm_alaw,pcm_mulaw,adpcm_ms,adpcm_ima_wav,mp2float,dca
PARSERS=mpeg4video,mjpeg,vp8,vp9,av1,mpegaudio,flac,vorbis,opus,ac3,mpegvideo,dca
# What we write: VP9 + Opus in WebM, Opus in Ogg.
ENCODERS=libvpx_vp9,libopus
MUXERS=webm,ogg
# The WebM muxer runs VP9 through this, but configure doesn't select it for "webm".
BSFS=vp9_superframe
FILTERS=scale,format,aresample,aformat,null,anull

CONFIGURE=(
  --target-os=mingw32 --arch=x86_64 --cross-prefix=$HOST- --pkg-config=pkg-config
  --pkg-config-flags=--static --extra-ldflags=-static --prefix="$WORK/ffmpeg-install"
  --enable-static --disable-shared
  --disable-everything --disable-autodetect --disable-network --disable-doc
  --disable-ffplay --disable-ffprobe --disable-avdevice --disable-debug --enable-small
  --enable-w32threads --enable-libvpx --enable-libopus --enable-libdav1d
  # "fd" reads "-" (stdin) since ffmpeg 9; "pipe" writes -progress pipe:1.
  --enable-protocol=file,pipe,fd
  --enable-demuxer=$DEMUXERS
  --enable-decoder=$VIDEO_DECODERS,$AUDIO_DECODERS
  --enable-parser=$PARSERS
  --enable-encoder=$ENCODERS
  --enable-muxer=$MUXERS
  --enable-bsf=$BSFS
  --enable-filter=$FILTERS
  --extra-version=sastra.$SASTRA_BUILD
)
( cd "$SRC/ffmpeg"
  ./configure "${CONFIGURE[@]}"
  make -j"$JOBS"
  $HOST-strip -o "$OUT/ffmpeg.exe" ffmpeg.exe )

echo "== licences and sources"
cp "$SRC/ffmpeg/COPYING.LGPLv2.1" "$OUT/licences/ffmpeg-LGPLv2.1.txt"
cp "$SRC/ffmpeg/LICENSE.md"       "$OUT/licences/ffmpeg-LICENSE.md"
cp "$SRC/libvpx/LICENSE"          "$OUT/licences/libvpx-BSD.txt"
cp "$SRC/libvpx/PATENTS"          "$OUT/licences/libvpx-PATENTS.txt"
cp "$SRC/opus/COPYING"            "$OUT/licences/opus-BSD.txt"
cp "$SRC/dav1d/COPYING"           "$OUT/licences/dav1d-BSD.txt"
cp "$SRC"/ffmpeg.tar.xz "$SRC"/libvpx.tar.gz "$SRC"/opus.tar.gz "$SRC"/dav1d.tar.xz "$OUT/sources/"
cp "$HERE/build.sh" "$HERE/versions.sh" "$OUT/sources/"
{
  echo "ffmpeg $FFMPEG_VERSION (sastra.$SASTRA_BUILD), built $(date -u +%Y-%m-%d)"
  echo "libvpx $LIBVPX_VERSION"
  echo "opus $OPUS_VERSION"
  echo "dav1d $DAV1D_VERSION"
  echo "Compiler: $($HOST-gcc --version | head -1)"
  echo
  echo "configure ${CONFIGURE[*]}"
} > "$OUT/BUILD.txt"

ls -l "$OUT/ffmpeg.exe"
