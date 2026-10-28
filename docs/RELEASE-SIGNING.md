# Signing and notarizing a macOS release

The tag workflow builds AUv2, VST3, and CLAP for Apple Silicon and Intel and creates a **draft** GitHub release. Its archives are build artifacts, not distribution files. Keep the release draft until each of the six bundles is Developer ID signed, accepted by Apple's notary service, stapled, and repackaged. Replace the draft assets and checksums before publishing it.

This release uses the `Developer ID Application: Todd Greco (89YJQ4UB7V)` certificate and the `TRUCE_NOTARY` notarytool Keychain profile. The profile contains credentials in the local Keychain; do not put those credentials or an exported signing key in this repository. Verify the profile with `xcrun notarytool history --keychain-profile TRUCE_NOTARY`.

**Scripted:** `tools/release/sign_macos.sh <tag> <arm64|x86_64> [--upload]` performs the steps below for one architecture. It downloads that architecture’s CI archives and checks their checksums, signs, notarizes (waiting for `Accepted`), staples and validates each bundle, repackages with `LICENSE` and `THIRD-PARTY-NOTICES.md`, extracts and re-verifies each final zip, and writes `SHA256-macos-<arch>.txt`. Output lands in `~/current_work/junglizer-release/<tag>/<arch>/final`, never in `target/`. With `--upload` it also replaces the draft release’s assets with `gh release upload --clobber`. Run it once per architecture, then check the uploads and publish the release as the latest.

For each architecture, unpack the three CI archives, then sign each `.component`, `.vst3`, and `.clap` bundle with hardened runtime and a secure timestamp:

```sh
codesign --force --options runtime --timestamp \
  --sign 'Developer ID Application: Todd Greco (89YJQ4UB7V)' Junglizer.component
codesign --verify --deep --strict Junglizer.component
```

Repeat for the other formats. Submit a ZIP containing the three signed bundles with `xcrun notarytool submit signed-bundles.zip --keychain-profile TRUCE_NOTARY --wait`. Continue only if the result is `Accepted`; use `xcrun notarytool log <submission-id> --keychain-profile TRUCE_NOTARY` to diagnose a rejection. Staple and validate each bundle:

```sh
xcrun stapler staple Junglizer.component
xcrun stapler validate Junglizer.component
codesign --verify --deep --strict Junglizer.component
```

Repeat for VST3 and CLAP. **Create the release ZIPs after stapling**: the ZIP itself cannot carry a stapled ticket. Put `LICENSE` and `THIRD-PARTY-NOTICES.md` (from the repo root) in each ZIP next to the bundle, so all three sit at the zip root, as in the CI-built drafts. Extract each final ZIP into a clean directory and run `stapler validate` and `codesign --verify --deep --strict` on the extracted bundle. Keep the CI asset names (`Junglizer-macos-{arm64,x86_64}-{component,vst3,clap}.zip`; the site links to `releases/latest/download/<name>`, so names must not change), regenerate `SHA256-macos-arm64.txt` and `SHA256-macos-x86_64.txt`, replace the draft assets, and publish the release only after checking all six uploads.

For `v0.9.0-beta.1`, Apple accepted the arm64 submission `36b6bb1b-0fd0-4734-837c-b689859b1762` and Intel submission `4fa786a8-8e72-4769-93c9-d80c29f626ea`. Every bundle was stapled and validated before distribution.

## Windows and Linux zips are unsigned

The same tag workflow (and a manual dispatch) also builds `Junglizer-windows-x86_64-{vst3,clap}.zip` and `Junglizer-linux-x86_64-{vst3,clap}.zip`, with `SHA256-windows.txt` and `SHA256-linux.txt`. These are **not** code signed. Windows SmartScreen may warn about the unsigned binaries, and nothing here needs Apple notarization. Leave them in the draft release as built; only the six macOS archives get replaced by the steps above. Asset names carry no version so `releases/latest/download/<name>` always serves the newest build; publish each release as the latest (not a pre-release) for that to work.
