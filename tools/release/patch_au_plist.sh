#!/usr/bin/env bash
# Fixes the two things cargo-truce 6.3 hardcodes in the AUv2 Info.plist,
# then re-signs the bundle ad hoc.
#
# - `sandboxSafe = true`. Logic Pro takes that at its word and runs the
#   plug-in in a sandboxed AUHostingService, where opening a break in
#   Desktop, Documents or Downloads fails with EPERM (the kernel logs
#   `Sandbox: AUHostingServiceXPC_arrow deny(1) file-read-data`), even for a
#   file the user just picked or dropped. Junglizer reads breaks by path, so
#   it isn't sandbox-safe; with the key false, Logic hosts it unsandboxed.
# - `version = 65536`, which Logic shows as 1.0.0. The AU version packs
#   major.minor.patch as 0xMMMMmmpp; this sets it, CFBundleVersion and
#   CFBundleShortVersionString from Cargo.toml (0.9.3-beta.4 -> 0x000903,
#   "0.9.3"). Hosts recall the AU by type/subtype/manufacturer, not version.
#
# Run it on every built .component before packing or installing:
#   tools/release/patch_au_plist.sh target/bundles/Junglizer.component
# The ad-hoc signature keeps the bundle valid until the release signing
# (tools/release/sign_macos.sh) replaces it with the Developer ID one.
set -euo pipefail

bundle="${1:?path to a .component bundle}"
plist="$bundle/Contents/Info.plist"
root="$(cd "$(dirname "$0")/../.." && pwd)"
test -f "$plist" || { echo "no Info.plist in $bundle" >&2; exit 1; }

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)"
[[ "$version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+) ]] ||
  { echo "can't read the version from Cargo.toml" >&2; exit 1; }
major="${BASH_REMATCH[1]}" minor="${BASH_REMATCH[2]}" patch="${BASH_REMATCH[3]}"
(( minor < 256 && patch < 256 )) || { echo "version $version doesn't fit an AU version" >&2; exit 1; }
short="$major.$minor.$patch"
au_version=$(( (major << 16) | (minor << 8) | patch ))

# Sets `key` to `value` of `type`, adding the key if it's missing.
set_key() {
  local key="$1" type="$2" value="$3"
  /usr/libexec/PlistBuddy -c "Set $key $value" "$plist" 2>/dev/null ||
    /usr/libexec/PlistBuddy -c "Add $key $type $value" "$plist"
}

set_key :CFBundleShortVersionString string "$short"
set_key :CFBundleVersion string "$short"
count="$(/usr/libexec/PlistBuddy -c 'Print :AudioComponents' "$plist" | grep -c '^    Dict {')"
for ((i = 0; i < count; i++)); do
  set_key ":AudioComponents:$i:sandboxSafe" bool false
  set_key ":AudioComponents:$i:version" integer "$au_version"
done

codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
for ((i = 0; i < count; i++)); do
  safe="$(/usr/libexec/PlistBuddy -c "Print :AudioComponents:$i:sandboxSafe" "$plist")"
  got="$(/usr/libexec/PlistBuddy -c "Print :AudioComponents:$i:version" "$plist")"
  [ "$safe" = false ] || { echo "sandboxSafe is still $safe in $bundle" >&2; exit 1; }
  [ "$got" = "$au_version" ] || { echo "AU version is $got, not $au_version" >&2; exit 1; }
done
echo "$bundle: version $short (AU $au_version), sandboxSafe false, $count component(s)"
