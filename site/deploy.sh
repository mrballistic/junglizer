#!/usr/bin/env bash
# Publish the getjunglizer.com site.
# Pulls release zips from the private GitHub repo using YOUR local `gh` login,
# then rsyncs the site (plus downloads/) to the Ubuntu server.
#
# Usage:  ./site/deploy.sh [tag]        e.g. ./site/deploy.sh v0.9.0-beta.1
# Needs:  gh (logged in: `gh auth status`), rsync, SSH access to the server.
set -euo pipefail

TAG="${1:-v0.9.0-beta.1}"
REPO="mrballistic/junglizer"
SERVER="${SERVER:-toddgreco@mrballistic.com}"
REMOTE_DIR="${REMOTE_DIR:-/var/www/getjunglizer.com}"

SITE_DIR="$(cd "$(dirname "$0")" && pwd)"
DL_DIR="$SITE_DIR/downloads"
mkdir -p "$DL_DIR"

echo "→ Downloading $TAG release assets from $REPO"
gh release download "$TAG" --repo "$REPO" --dir "$DL_DIR" \
  --pattern '*.zip' --pattern 'SHA256-*.txt' --clobber

echo "→ Verifying checksums"
( cd "$DL_DIR" && for f in SHA256-*.txt; do shasum -a 256 -c "$f"; done )

echo "→ Uploading to $SERVER:$REMOTE_DIR"
rsync -avz --delete --chmod=D755,F644 \
  --exclude 'deploy.sh' --exclude '.DS_Store' \
  "$SITE_DIR/" "$SERVER:$REMOTE_DIR/"

echo "✓ Done. https://getjunglizer.com"
