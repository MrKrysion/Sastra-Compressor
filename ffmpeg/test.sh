#!/usr/bin/env bash
# Checks a built ffmpeg.exe against every input format the app accepts.
#
#   ./test.sh path/to/ffmpeg.exe
#
# Makes one small sample per format with the system's own ffmpeg (which has
# every encoder), converts each with ffmpeg.exe using the same arguments as
# the app (src-tauri/src/ffmpeg.rs), and checks the result with ffprobe.
# Runs ffmpeg.exe under Wine on Linux, or directly on Windows.
#
# ffmpeg.exe has no patented codecs. In the app, the operating system decodes
# H.264, HEVC, AAC, and E-AC-3 and pipes raw NV12 frames and PCM sound into it;
# here the system's ffmpeg stands in for the operating system.
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
  got="$(ffprobe -v error -show_entries stream=codec_name -of csv=p=0 "$3" 2>/dev/null | sort | tr '\n' ' ')"
  if [ "$got" = "$2 " ]; then echo "  ok    $1"; else echo "  FAIL  $1 (got: $got, expected: $2)"; fail=1; fi
}
run() { "${RUN[@]}" "$EXE" -hide_banner -nostdin -nostats -loglevel error -y "$@" 2>&1 | tail -3; }
# What the app writes for a video, into VIDEO. 0 = keep the width (here: 322 → 322,
# height made even). The sound is input 0, or 1 when the frames come in through a pipe.
video() { # width sound-input
  VIDEO=(-map 0:v:0 -map "$2:a:0?" -sn -dn -map_chapters -1
    -vf "scale='trunc(min($1,iw)/2)*2':-2,format=yuv420p"
    -c:v libvpx-vp9 -crf 33 -b:v 0 -deadline good -cpu-used 4 -row-mt 1
    -c:a libopus -b:a 96k -vbr on -cues_to_front 1 -f webm)
}

echo "Checking that no patented codec is built in"
for c in h264 hevc aac aac_latm eac3 prores dnxhd; do
  if "${RUN[@]}" "$EXE" -hide_banner -decoders 2>/dev/null | grep -qE "^ [A-Z.]{6} $c "; then echo "  FAIL  decoder $c"; fail=1; fi
done
for c in libx264 aac h264_mf aac_mf; do
  if "${RUN[@]}" "$EXE" -hide_banner -encoders 2>/dev/null | grep -qE "^ [A-Z.]{6} $c "; then echo "  FAIL  encoder $c"; fail=1; fi
done
[ $fail = 0 ] && echo "  ok    none"

echo "Converting videos"
video iw 0
for f in vp9-opus.webm vp8-vorbis.webm av1-opus.mkv mpeg4-mp3.avi mjpeg-pcm.avi raw-silent.avi \
         qtrle.mov huffyuv.avi ffvhuff.avi utvideo.avi ffv1.mkv mpeg2-mp2.mkv mpeg1-mp2.mkv divx3.avi wmv2.avi; do
  out="out-$f.webm"
  run -i "$f" "${VIDEO[@]}" "$out" || true
  if [ "$f" = raw-silent.avi ]; then check "$f" "vp9" "$out"; else check "$f" "opus vp9" "$out"; fi
done

# Read by the operating system in the app: raw frames on stdin, the sound as a WAV file.
echo "Converting videos the operating system reads"
video iw 1
for f in h264-aac.mp4 h264-aac.m4v hevc-ac3.mkv h264-eac3.mkv h264-dts.mkv h264-alaw.mov h264-mulaw.mov; do
  out="out-$f.webm"
  size="$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$f")"
  rate="$(ffprobe -v error -select_streams v:0 -show_entries stream=avg_frame_rate -of csv=p=0 "$f")"
  gen -i "$f" -map 0:a:0 -c:a pcm_f32le sound.wav
  ffmpeg -nostdin -loglevel error -i "$f" -map 0:v:0 -f rawvideo -pix_fmt nv12 - \
    | "${RUN[@]}" "$EXE" -hide_banner -nostats -loglevel error -y \
        -f rawvideo -pix_fmt nv12 -video_size "$size" -framerate "$rate" -i - -i sound.wav "${VIDEO[@]}" "$out" 2>&1 | tail -3 || true
  check "$f" "opus vp9" "$out"
done

# What can't be read without a patented decoder must fail clearly, not quietly.
echo "Refusing what needs a patented decoder"
video iw 0
for f in h264-aac.mp4 prores-pcm.mov dnxhr-pcm.mov a.aac; do
  if [ "$f" = a.aac ]; then args=(-map 0:a:0 -c:a libopus -f ogg); else args=("${VIDEO[@]}"); fi
  msg="$(run -i "$f" "${args[@]}" "refused-$f" || true)"
  if [ ! -s "refused-$f" ] && grep -qiE "no decoder found|decoder.*not found" <<<"$msg"; then
    echo "  ok    $f"
  else
    echo "  FAIL  $f (${msg:-no error})"; fail=1
  fi
done

echo "Converting sound"
for f in *.wav *.w64 *.flac *.mp3 *.ogg *.oga *.opus *.aiff *.aif alac.m4a; do
  case "$f" in sound.wav) continue ;; esac
  out="out-$f.ogg"
  run -i "$f" -map 0:a:0 -vn -sn -dn -map_chapters -1 -c:a libopus -b:a 96k -vbr on -f ogg "$out" || true
  check "$f" "opus" "$out"
done

# shrinking a video to a smaller width
video 200 0
run -i vp9-opus.webm "${VIDEO[@]}" small.webm
size="$(ffprobe -v error -select_streams v -show_entries stream=width,height -of csv=p=0 small.webm)"
if [ "$size" = "200,114" ]; then echo "  ok    resize to 200 px wide"; else echo "  FAIL  resize (got $size)"; fail=1; fi

[ $fail = 0 ] && echo "All formats passed." || { echo "Some formats failed."; exit 1; }
