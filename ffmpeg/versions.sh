# Pinned versions of everything that goes into ffmpeg.exe.
# Change a version here, update its checksum, and CI rebuilds ffmpeg.

FFMPEG_VERSION=8.0.1
FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
FFMPEG_SHA256=05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41

# x264 has no release tarballs; we pin a commit of its "stable" branch.
# Git checks the commit by its hash, so no separate checksum is needed.
X264_REPO=https://code.videolan.org/videolan/x264.git
X264_COMMIT=b35605ace3ddf7c1a5d67a2eb553f034aef41d55

OPUS_VERSION=1.5.2
OPUS_URL="https://github.com/xiph/opus/releases/download/v${OPUS_VERSION}/opus-${OPUS_VERSION}.tar.gz"
OPUS_SHA256=65c1d2f78b9f2fb20082c38cbe47c951ad5839345876e46941612ee87f9a7ce1

DAV1D_VERSION=1.5.1
DAV1D_URL="https://downloads.videolan.org/pub/videolan/dav1d/${DAV1D_VERSION}/dav1d-${DAV1D_VERSION}.tar.xz"
DAV1D_SHA256=401813f1f89fa8fd4295805aa5284d9aed9bc7fc1fdbe554af4292f64cbabe21

# Bump this when the build changes but the versions above don't.
SASTRA_BUILD=2
