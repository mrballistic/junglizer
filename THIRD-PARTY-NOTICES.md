# Third-party notices

Junglizer
Copyright © 2026 Todd Greco

This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with this program (`LICENSE`). If not, see <https://www.gnu.org/licenses/>.

`LICENSE` ships in every release zip next to this file and the plugin bundle. Complete source code is at https://github.com/mrballistic/junglizer.

Junglizer is built on the work below, whose own licenses still apply to those parts.

## Built-in breaks

Built-in breaks played on the Salamander Drumkit by Alexander Holm.

- License: Creative Commons Attribution-ShareAlike 3.0 (CC BY-SA 3.0), https://creativecommons.org/licenses/by-sa/3.0/
- Source: Salamander Drumkit v1, https://download.linuxaudio.org/musical-instrument-libraries/sfz/salamander_drumkit_v1.tar.7z (archive SHA-256 `a576ff79fcc33b53d8a373f459277e6727bd4e68d5383b9d69b1d9e0ec585329`)
- The pattern was programmed from scratch for Junglizer. The four bar renders embedded in the plugin (`sample/174_JunglizerAmen_Bar_01.wav` to `_04.wav`) are adaptations of the Salamander samples: rendered from Salamander Drumkit one-shots (one sample per hit), arranged to a pattern programmed from scratch, with EQ and level shaping.
- Share-alike applies to the sample set only: the adapted drum renders are offered under CC BY-SA 3.0. It does not extend to the Junglizer plugin code, which is licensed separately.

See `sample/LICENSE.md` in the source repository for the full terms.

## truce-egui (modified)

Junglizer uses a vendored copy of `truce-egui`, which is dual-licensed under Apache-2.0 or MIT. The license texts are in the source repository at `vendor/truce-egui/LICENSE-APACHE` and `vendor/truce-egui/LICENSE-MIT`.

Modification: the vendored copy forwards file drag-and-drop events from the host window to the editor, so a break can be dropped onto a slot. Upstream does not do this.

## Other Rust dependencies

Junglizer’s other Rust dependencies are licensed under MIT, Apache-2.0 or MPL-2.0 (Symphonia, the audio decoder, is MPL-2.0). `Cargo.lock` in the source repository lists the exact versions.
