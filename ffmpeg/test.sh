#!/usr/bin/env bash
# Checks a built ffmpeg.exe against every input format the app accepts.
#
#   ./test.sh path/to/ffmpeg.exe
#
# Makes one small sample per format with the system's own ffmpeg (which has
# every encoder), converts each with ffmpeg.exe using the same arguments as
# the app (src-tauri/src/ffmpeg.rs), and checks the result with ffprobe.
# Runs ffmpeg.exe under Wine on Linux, or directly on Windows.
set -euo pipefail

EXE="$(realpath "$1")"
RUN=()
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) ;; *) RUN=("$(command -v wine || command -v wine64)") ;; esac
export WINEDEBUG=-all

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
cd "$T"
gen() { ffmpeg -nostdin -hide_banner -loglevel error -y "$@"; }

V=(-f lavfi -i testsrc2=size=322x182:rate=24:duration=2)   # odd half-width on purpose
A=(-f lavfi -i sine=frequency=440:sample_rate=44100:duration=2)
A6=(-f lavfi -i "sine=frequency=440:duration=2,aformat=channel_layouts=5.1")

echo "Making samples"
# video
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a aac -shortest h264-aac.mp4
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a aac -shortest h264-aac.m4v
gen "${V[@]}" "${A6[@]}" -c:v libx265 -x265-params log-level=error -c:a ac3 -shortest hevc-ac3.mkv
gen "${V[@]}" "${A[@]}" -c:v libvpx-vp9 -c:a libopus -shortest vp9-opus.webm
gen "${V[@]}" "${A[@]}" -c:v libvpx -c:a libvorbis -shortest vp8-vorbis.webm
gen "${V[@]}" "${A[@]}" -c:v libaom-av1 -cpu-used 8 -c:a libopus -shortest av1-opus.mkv
gen "${V[@]}" "${A[@]}" -c:v prores_ks -c:a pcm_s24le -shortest prores-pcm.mov
gen "${V[@]}" "${A[@]}" -c:v dnxhd -profile:v dnxhr_lb -pix_fmt yuv422p -c:a pcm_s16le -shortest dnxhr-pcm.mov
gen "${V[@]}" "${A[@]}" -c:v mpeg4 -c:a mp3 -shortest mpeg4-mp3.avi
gen "${V[@]}" "${A[@]}" -c:v mjpeg -c:a pcm_s16le -shortest mjpeg-pcm.avi
gen "${V[@]}" -c:v rawvideo -pix_fmt bgr24 raw-silent.avi
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a eac3 -shortest h264-eac3.mkv
gen "${V[@]}" "${A[@]}" -c:v qtrle -c:a pcm_s16le -shortest qtrle.mov
gen "${V[@]}" "${A[@]}" -c:v huffyuv -c:a pcm_s16le -shortest huffyuv.avi
gen "${V[@]}" "${A[@]}" -c:v ffvhuff -c:a pcm_s16le -shortest ffvhuff.avi
gen "${V[@]}" "${A[@]}" -c:v utvideo -pix_fmt yuv422p -c:a pcm_s16le -shortest utvideo.avi
gen "${V[@]}" "${A[@]}" -c:v ffv1 -c:a flac -shortest ffv1.mkv
gen "${V[@]}" "${A[@]}" -c:v mpeg2video -c:a mp2 -shortest mpeg2-mp2.mkv
gen "${V[@]}" "${A[@]}" -c:v mpeg1video -c:a mp2 -shortest mpeg1-mp2.mkv
gen "${V[@]}" "${A[@]}" -c:v msmpeg4v3 -c:a mp3 -shortest divx3.avi
gen "${V[@]}" "${A[@]}" -c:v wmv2 -c:a mp3 -shortest wmv2.avi
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a dca -strict -2 -shortest h264-dts.mkv
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a pcm_alaw -shortest h264-alaw.mov
gen "${V[@]}" "${A[@]}" -c:v libx264 -c:a pcm_mulaw -shortest h264-mulaw.mov
# audio
gen "${A[@]}" -c:a pcm_s8 s8.aif
gen "${A[@]}" -c:a pcm_alaw alaw.wav
gen "${A[@]}" -c:a pcm_mulaw mulaw.wav
gen "${A[@]}" -c:a adpcm_ms adpcm-ms.wav
gen "${A[@]}" -c:a adpcm_ima_wav adpcm-ima.wav
gen "${A[@]}" -c:a pcm_s16le s16.wav
gen "${A[@]}" -c:a pcm_s24le s24.wav
gen "${A[@]}" -c:a pcm_f32le f32.wav
gen "${A[@]}" -c:a pcm_u8 u8.wav
gen "${A[@]}" -c:a pcm_s16le long.w64
gen "${A[@]}" -c:a flac a.flac
gen "${A[@]}" -f lavfi -i color=red:size=64x64:duration=1 -map 0 -map 1 -frames:v 1 -c:a libmp3lame -c:v mjpeg -disposition:v attached_pic cover.mp3
gen "${A[@]}" -c:a libvorbis a.ogg
gen "${A[@]}" -c:a libvorbis a.oga
gen "${A[@]}" -c:a libopus a.opus
gen "${A[@]}" -c:a aac a.m4a
gen "${A[@]}" -c:a alac alac.m4a
gen "${A[@]}" -c:a aac a.aac
gen "${A[@]}" -c:a pcm_s16be a.aiff
gen "${A[@]}" -c:a pcm_s24be a.aif
gen "${A6[@]}" -c:a pcm_s16le surround.wav

fail=0
check() { # name expected-codecs output
  local got
  got="$(ffprobe -v error -show_entries stream=codec_name -of csv=p=0 "$3" | sort | tr '\n' ' ')"
  if [ "$got" = "$2 " ]; then echo "  ok    $1"; else echo "  FAIL  $1 (got: $got, expected: $2)"; fail=1; fi
}
run() { "${RUN[@]}" "$EXE" -hide_banner -nostdin -nostats -loglevel error -y "$@" 2>&1 | tail -3; }

echo "Converting"
for f in *.mp4 *.m4v *.mkv *.webm *.mov *.avi; do
  out="out-$f.mp4"
  # same as the app; 0 = keep the width (here: 322 → 322, height made even)
  run -i "$f" -map 0:v:0 -map "0:a:0?" -sn -dn -map_chapters -1 \
    -vf "scale='trunc(min(iw,iw)/2)*2':-2,format=yuv420p" \
    -c:v libx264 -preset slow -crf 26 -c:a aac -b:a 128k -movflags +faststart -f mp4 "$out" || true
  if [ "$f" = raw-silent.avi ]; then check "$f" "h264" "$out"; else check "$f" "aac h264" "$out"; fi
done
for f in *.wav *.w64 *.flac *.mp3 *.ogg *.oga *.opus *.m4a *.aac *.aiff *.aif; do
  out="out-$f.ogg"
  run -i "$f" -map 0:a:0 -vn -sn -dn -map_chapters -1 -c:a libopus -b:a 96k -vbr on -f ogg "$out" || true
  check "$f" "opus" "$out"
done
# shrinking a video to a smaller width
run -i h264-aac.mp4 -map 0:v:0 -map "0:a:0?" -vf "scale='trunc(min(200,iw)/2)*2':-2,format=yuv420p" \
  -c:v libx264 -preset slow -crf 26 -c:a aac -b:a 128k -f mp4 small.mp4
size="$(ffprobe -v error -select_streams v -show_entries stream=width,height -of csv=p=0 small.mp4)"
if [ "$size" = "200,114" ]; then echo "  ok    resize to 200 px wide"; else echo "  FAIL  resize (got $size)"; fail=1; fi

[ $fail = 0 ] && echo "All formats passed." || { echo "Some formats failed."; exit 1; }
