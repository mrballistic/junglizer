#!/usr/bin/env bash
# Publish the getjunglizer.com site.
# Downloads are no longer self-hosted: the site links to
# https://github.com/mrballistic/junglizer/releases/latest/download/<name>,
# so no gh login (and no release zips) is needed here. This just rsyncs the
# site to the Ubuntu server. downloads/ is excluded with --delete-excluded,
# so any old self-hosted zips on the server are removed.
#
# Usage:  ./site/deploy.sh
# Needs:  rsync, SSH access to the server.
set -euo pipefail

SERVER="${SERVER:-toddgreco@mrballistic.com}"
REMOTE_DIR="${REMOTE_DIR:-/var/www/getjunglizer.com}"

SITE_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "→ Uploading to $SERVER:$REMOTE_DIR"
# A plain --exclude would *protect* downloads/ from --delete on the server;
# --delete-excluded makes --delete remove it there too.
rsync -avz --delete --delete-excluded --chmod=D755,F644 \
  --exclude 'deploy.sh' --exclude '.DS_Store' --exclude 'downloads/' \
  "$SITE_DIR/" "$SERVER:$REMOTE_DIR/"

echo "✓ Done. https://getjunglizer.com"
