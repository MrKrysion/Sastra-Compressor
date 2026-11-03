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
#   sources/              the exact source code used (GPL: shipped with releases)
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

echo "== x264"
if [ ! -f "$SRC/x264.tar.xz" ]; then
  rm -rf "$SRC/x264-git"; git init -q "$SRC/x264-git"
  git -C "$SRC/x264-git" fetch -q --depth 1 "$X264_REPO" "$X264_COMMIT"
  [ "$(git -C "$SRC/x264-git" rev-parse FETCH_HEAD)" = "$X264_COMMIT" ]
  git -C "$SRC/x264-git" archive --prefix=x264/ FETCH_HEAD | xz -9 > "$SRC/x264.tar.xz"
  rm -rf "$SRC/x264-git"
fi
unpack x264.tar.xz x264
( cd "$SRC/x264"
  ./configure --host=$HOST --cross-prefix=$HOST- --prefix="$PREFIX" \
    --enable-static --disable-cli --disable-opencl --disable-lavf \
    --disable-swscale --disable-ffms --disable-gpac --disable-lsmash --bit-depth=8
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
# Keep this list in step with src/lib/formats.ts in the app.
DEMUXERS=mov,matroska,avi,mp3,wav,w64,flac,ogg,aac,aiff
VIDEO_DECODERS=h264,hevc,prores,dnxhd,mpeg4,mjpeg,vp8,vp9,libdav1d,rawvideo
AUDIO_DECODERS=aac,mp3float,flac,vorbis,opus,alac,ac3,eac3,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_f64le,pcm_s16be,pcm_s24be,pcm_s32be,pcm_u8
PARSERS=h264,hevc,mpeg4video,mjpeg,dnxhd,vp8,vp9,av1,aac,mpegaudio,flac,vorbis,opus,ac3
# What we write: H.264 + AAC in MP4, Opus in Ogg; "null" is used to read durations.
ENCODERS=libx264,aac,libopus
MUXERS=mp4,ogg,null
FILTERS=scale,format,aresample,aformat,null,anull

CONFIGURE=(
  --target-os=mingw32 --arch=x86_64 --cross-prefix=$HOST- --pkg-config=pkg-config
  --pkg-config-flags=--static --extra-ldflags=-static --prefix="$WORK/ffmpeg-install"
  --enable-gpl --enable-static --disable-shared
  --disable-everything --disable-autodetect --disable-network --disable-doc
  --disable-ffplay --disable-ffprobe --disable-avdevice --disable-debug --enable-small
  --enable-w32threads --enable-libx264 --enable-libopus --enable-libdav1d
  --enable-protocol=file,pipe
  --enable-demuxer=$DEMUXERS
  --enable-decoder=$VIDEO_DECODERS,$AUDIO_DECODERS
  --enable-parser=$PARSERS
  --enable-encoder=$ENCODERS
  --enable-muxer=$MUXERS
  --enable-filter=$FILTERS
  --extra-version=sastra.$SASTRA_BUILD
)
( cd "$SRC/ffmpeg"
  ./configure "${CONFIGURE[@]}"
  make -j"$JOBS"
  $HOST-strip -o "$OUT/ffmpeg.exe" ffmpeg.exe )

echo "== licences and sources"
cp "$SRC/ffmpeg/COPYING.GPLv2" "$OUT/licences/ffmpeg-GPLv2.txt"
cp "$SRC/ffmpeg/LICENSE.md"    "$OUT/licences/ffmpeg-LICENSE.md"
cp "$SRC/x264/COPYING"         "$OUT/licences/x264-GPLv2.txt"
cp "$SRC/opus/COPYING"         "$OUT/licences/opus-BSD.txt"
cp "$SRC/dav1d/COPYING"        "$OUT/licences/dav1d-BSD.txt"
cp "$SRC"/ffmpeg.tar.xz "$SRC"/x264.tar.xz "$SRC"/opus.tar.gz "$SRC"/dav1d.tar.xz "$OUT/sources/"
cp "$HERE/build.sh" "$HERE/versions.sh" "$OUT/sources/"
{
  echo "ffmpeg $FFMPEG_VERSION (sastra.$SASTRA_BUILD), built $(date -u +%Y-%m-%d)"
  echo "x264 $X264_COMMIT"
  echo "opus $OPUS_VERSION"
  echo "dav1d $DAV1D_VERSION"
  echo "Compiler: $($HOST-gcc --version | head -1)"
  echo
  echo "configure ${CONFIGURE[*]}"
} > "$OUT/BUILD.txt"

ls -l "$OUT/ffmpeg.exe"
