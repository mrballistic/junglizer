# Junglizer 0.9 beta 1

The four-bar break instrument now ships as AUv2, VST3, and CLAP for Apple Silicon and Intel Macs. Each download contains one format; the four supplied breaks are embedded in every plugin bundle.

The editor has a source waveform with movable slice markers, a four-bar player with placement previews and ghost waveforms, live phrase variations, freeze and lock controls, MIDI export, a Thin/Dry/Thicc crossfader, root-tuned 808 reinforcement, and an Output knob. The graffiti wordmark and compact player layout are new in this beta.

Download the archive matching your Mac and host, unzip it, and copy the bundle to the matching directory under `~/Library/Audio/Plug-Ins/` (`Components`, `VST3`, or `CLAP`). Restart the host so it scans the plugin. The archives are ad-hoc signed for beta testing; they are not Apple notarized. You may need to allow the plugin in macOS security settings.

This beta uses the included 174 BPM breaks. Fit to length changes pitch with speed; pitch-preserving stretch is planned. The host tempo controls playback speed. MIDI export writes to `~/Music/Junglizer/Exports` and reveals the file in Finder. Direct drag from the plugin to Logic is not implemented.

CI runs Rust formatting and unit tests, then builds and packages all three formats for both Mac architectures. The release includes SHA-256 checksums. Native editor screenshot fixtures and the local AU validator passed before tagging; in-host testing remains important for this beta.
