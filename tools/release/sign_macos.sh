#!/usr/bin/env bash
# Sign, notarize, staple and repackage the macOS release archives for one
# architecture, following docs/RELEASE-SIGNING.md, then replace them in the
# draft GitHub release.
#
# Usage: tools/release/sign_macos.sh <tag> <arm64|x86_64> [--upload]
# Needs the Developer ID certificate and the TRUCE_NOTARY notarytool profile
# in the login Keychain, and gh access to mrballistic/junglizer.
# Work happens in ~/current_work/junglizer-release/<tag>/<arch> (never target/).
set -euo pipefail

tag="${1:?tag}"
arch="${2:?arch (arm64 or x86_64)}"
upload="${3:-}"
repo="mrballistic/junglizer"
identity="Developer ID Application: Todd Greco (89YJQ4UB7V)"
root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$HOME/current_work/junglizer-release/$tag/$arch"
export GH_TOKEN="${GH_TOKEN:-$(gh auth token -u mrballistic)}"

mkdir -p "$work"
cd "$work"
rm -rf ci unpacked final notary
mkdir -p ci unpacked final notary

echo "== download CI archives ($arch)"
gh release download "$tag" --repo "$repo" --dir ci \
  --pattern "Junglizer-macos-$arch-*.zip" --pattern "SHA256-macos-$arch.txt"
(cd ci && shasum -a 256 -c "SHA256-macos-$arch.txt")

for format in component vst3 clap; do
  mkdir -p "unpacked/$format"
  ditto -x -k "ci/Junglizer-macos-$arch-$format.zip" "unpacked/$format"
  bundle="unpacked/$format/Junglizer.$format"
  test -d "$bundle"
  # A sandbox-safe AU can't open breaks in Desktop, Documents or Downloads
  # in Logic; CI clears the flag (tools/release/patch_au_plist.sh).
  if [ "$format" = component ] &&
    [ "$(/usr/libexec/PlistBuddy -c 'Print :AudioComponents:0:sandboxSafe' "$bundle/Contents/Info.plist")" != false ]; then
    echo "$bundle is marked sandboxSafe; refusing to sign" >&2
    exit 1
  fi
  echo "== sign $format"
  codesign --force --options runtime --timestamp --sign "$identity" "$bundle"
  codesign --verify --deep --strict "$bundle"
done

echo "== notarize ($arch)"
mkdir -p notary/bundles
for format in component vst3 clap; do
  ditto "unpacked/$format/Junglizer.$format" "notary/bundles/Junglizer.$format"
done
ditto -c -k --keepParent notary/bundles "notary/Junglizer-$tag-$arch-notary-submission.zip"
xcrun notarytool submit "notary/Junglizer-$tag-$arch-notary-submission.zip" \
  --keychain-profile TRUCE_NOTARY --wait --output-format json | tee notary/result.json
status="$(python3 -c 'import json,sys; print(json.load(open("notary/result.json"))["status"])')"
if [ "$status" != "Accepted" ]; then
  echo "Notarization status: $status" >&2
  exit 1
fi

for format in component vst3 clap; do
  bundle="unpacked/$format/Junglizer.$format"
  echo "== staple $format"
  xcrun stapler staple "$bundle"
  xcrun stapler validate "$bundle"
  codesign --verify --deep --strict "$bundle"
  stage="$(mktemp -d)"
  ditto "$bundle" "$stage/Junglizer.$format"
  cp "$root/LICENSE" "$root/THIRD-PARTY-NOTICES.md" "$stage/"
  ditto -c -k --sequesterRsrc "$stage" "final/Junglizer-macos-$arch-$format.zip"
  rm -rf "$stage"
  check="$(mktemp -d)"
  ditto -x -k "final/Junglizer-macos-$arch-$format.zip" "$check"
  test -f "$check/LICENSE" && test -f "$check/THIRD-PARTY-NOTICES.md"
  xcrun stapler validate "$check/Junglizer.$format"
  codesign --verify --deep --strict "$check/Junglizer.$format"
  rm -rf "$check"
done
(cd final && shasum -a 256 -- *.zip > "SHA256-macos-$arch.txt" && cat "SHA256-macos-$arch.txt")

if [ "$upload" = "--upload" ]; then
  echo "== replace draft assets ($arch)"
  gh release upload "$tag" --repo "$repo" --clobber final/*
fi
echo "== done ($arch): $work/final"
