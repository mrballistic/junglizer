# Signing and notarizing a macOS release

The tag workflow builds AUv2, VST3, and CLAP for Apple Silicon and Intel and creates a **draft** GitHub release. Its archives are build artifacts, not distribution files. Keep the release draft until each of the six bundles is Developer ID signed, accepted by Apple's notary service, stapled, and repackaged. Replace the draft assets and checksums before publishing it.

This release uses the `Developer ID Application: Todd Greco (89YJQ4UB7V)` certificate and the `TRUCE_NOTARY` notarytool Keychain profile. The profile contains credentials in the local Keychain; do not put those credentials or an exported signing key in this repository. Verify the profile with `xcrun notarytool history --keychain-profile TRUCE_NOTARY`.

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

Repeat for VST3 and CLAP. **Create the release ZIPs after stapling**: the ZIP itself cannot carry a stapled ticket. Extract each final ZIP into a clean directory and run `stapler validate` and `codesign --verify --deep --strict` on the extracted bundle. Regenerate `SHA256-arm64.txt` and `SHA256-x86_64.txt`, replace the draft assets, and publish the release only after checking all six uploads.

For `v0.9.0-beta.1`, Apple accepted the arm64 submission `36b6bb1b-0fd0-4734-837c-b689859b1762` and Intel submission `4fa786a8-8e72-4769-93c9-d80c29f626ea`. Every bundle was stapled and validated before distribution.
