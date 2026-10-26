# Junglizer wordmark

`junglizer-wordmark.png` is an original, transparent image generated for this
plugin on 2026-10-26. It uses 1990s UK jungle sleeve graffiti idioms suggested
by the user: interlocking angular letters, ivory fills, black ink outlines,
and a thin ivory keyline. It does not reproduce the supplied label's logo.
The user approved the initial direction and requested cleanup at the tops of
J and I. The shipped image is the second, cleaned version.

The final edit prompt is also embedded in the PNG's text metadata. The PNG is
embedded in the binary, decoded once when the editor first draws, and retained
as an egui texture. It is never loaded or decoded in the audio callback.
The header samples its transparent vertical margins using UV coordinates.

Initial generation prompt:

> Create an original wordmark for a desktop jungle music sampler plugin. Exact only text: JUNGLIZER (J U N G L I Z E R), one horizontal line. Authentic early 1990s UK jungle record sleeve graffiti idioms: tightly interlocking chunky angular hand-drawn block letters, energetic uneven baseline, slightly forward slant, confident thick black ink outlines with a thin warm-white outer keyline, warm-white letter fills, sparse angular cut marks and one or two tiny ink drips. Sharp, bold, readable at 260 px wide and 65 px high in an audio plugin header. Wide composition approximately 4:1 lettering, tightly cropped with small transparent margins. Genuine transparent background, no scene, no rectangle, no camo, no extra text, no existing record label logos, no gradients, no 3D rendering, no drop shadow. This is original lettering inspired by the visual language of 90s jungle sleeves, not a copy of a specific logo.

Final edit prompt:

> Polish this existing JUNGLIZER wordmark with only two local changes. Clean up the TOP of the J (first letter): smooth its upper cap contour and eliminate the little loose dangling artifact under the top-left cap. Clean up the TOP of the I (after L, before Z): remove the detached triangular spike/fragment floating above the stem and give the I a clean intentional short top cap aligned to the nearby letter tops. Preserve the exact spelling JUNGLIZER, all other letters, interlocking composition, warm ivory fills, black ink outlines, thin ivory keyline, graffiti character and bottom drips. Keep genuine transparent background. No other changes, no extra text.
