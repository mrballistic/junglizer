#!/bin/zsh
# Renders the "playing" editor state at 2x and encodes the site hero images
# and the README screenshot. Needs desktop GPU access (macOS), ffmpeg and
# cwebp (`brew install webp`). Renders into a scratch folder, so the tracked
# review fixture .impeccable/review/playing.png is left alone.
set -euo pipefail
cd "$(dirname "$0")/../.."
FFMPEG=${FFMPEG:-/opt/homebrew/bin/ffmpeg}
OUT=$(mktemp -d "${TMPDIR:-/tmp}/junglizer-render.XXXXXX")
trap 'rm -f "$OUT/playing.png"; rmdir "$OUT"' EXIT
JUNGLIZER_RENDER_DIR="$OUT" JUNGLIZER_RENDER_SCALE=2 JUNGLIZER_RENDER_ONLY=playing \
  cargo test render_editor_states -- --ignored --nocapture
SRC="$OUT/playing.png"
cp "$SRC" docs/screenshots/editor-playing.png
"$FFMPEG" -y -loglevel error -i "$SRC" -q:v 6 site/img/plugin.jpg
cwebp -quiet -q 80 -m 6 "$SRC" -o site/img/plugin.webp
sips -g pixelWidth -g pixelHeight site/img/plugin.jpg
ls -l docs/screenshots/editor-playing.png site/img/plugin.jpg site/img/plugin.webp
