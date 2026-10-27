#!/bin/zsh
# Renders the "playing" editor state at 2x and encodes the site hero images
# and the README screenshot. Needs desktop GPU access (macOS) and ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/../.."
FFMPEG=${FFMPEG:-/opt/homebrew/bin/ffmpeg}
JUNGLIZER_RENDER_SCALE=2 JUNGLIZER_RENDER_ONLY=playing \
  cargo test render_editor_states -- --ignored --nocapture
SRC=.impeccable/review/playing.png
cp "$SRC" docs/screenshots/editor-playing.png
"$FFMPEG" -y -loglevel error -i "$SRC" -q:v 6 site/img/plugin.jpg
cwebp -quiet -q 80 -m 6 "$SRC" -o site/img/plugin.webp
sips -g pixelWidth -g pixelHeight site/img/plugin.jpg
ls -l docs/screenshots/editor-playing.png site/img/plugin.jpg site/img/plugin.webp
