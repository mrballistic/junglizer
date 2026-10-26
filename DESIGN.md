---
name: Junglizer
description: A precise native break sampler with a UK jungle sleeve identity.
colors:
  canvas: "#171914"
  surface: "#22251D"
  well: "#10120F"
  edge: "#4C513D"
  ink: "#F4F1DE"
  muted: "#ABB29A"
  accent: "#D3E080"
  hover: "#394027"
  selection: "#4B5527"
  ghost: "#525B4D"
  playhead: "#FFFFFF"
  active-bar: "#3D4520"
  break-1: "#EE9757"
  break-2: "#69CDC1"
  break-3: "#97ADF1"
  break-4: "#DE8BBE"
typography:
  body:
    fontFamily: "egui proportional"
    fontSize: "13px"
  button:
    fontFamily: "egui proportional"
    fontSize: "13px"
  small:
    fontFamily: "egui proportional"
    fontSize: "12px"
  player-title:
    fontFamily: "egui proportional"
    fontSize: "19px"
rounded:
  widget: "3px"
  waveform: "5px"
  placement: "3px"
  footer: "4px"
spacing:
  item-x: "8px"
  item-y: "5px"
  inset: "14px"
  slice-gap: "3px"
  footer-inset: "10px"
components:
  button:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    typography: "{typography.button}"
    rounded: "{rounded.widget}"
    padding: "4px 8px"
  button-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.ink}"
    rounded: "{rounded.widget}"
  button-active:
    backgroundColor: "{colors.selection}"
    textColor: "{colors.ink}"
    rounded: "{rounded.widget}"
  source-waveform:
    backgroundColor: "{colors.well}"
    rounded: "{rounded.waveform}"
    height: "112px"
  player:
    backgroundColor: "{colors.well}"
    rounded: "{rounded.waveform}"
    height: "210px"
---

# Design System: Junglizer

## Overview

**Creative North Star: "UK jungle sleeve, working sampler"**

The user-supplied 1990s UK jungle sleeve and graffiti idioms inform the approved original JUNGLIZER wordmark, including its J/I cleanup. Olive-black, ivory, and lime give the native instrument its atmosphere. Functional controls remain legible and compact around the source-to-player workflow.

Product truth lives in [the UI design brief](docs/UI-DESIGN-BRIEF.md). Implemented tokens and interactions live in [src/ui.rs](src/ui.rs); this document captures that implementation rather than the brief's earlier incumbent palette. The later user-supplied [Opus playing-editor reference](docs/comps/Opus/Editor%20%C2%B7%20playing,%20placement%20selected.png) and companion tokens image informed more efficient grouping. The user preferred our original logo. References guide grouping and source identity, not glossy material imitation: this remains flat native egui. The revised grouping received a ship disposition after review of all 13 rendered states, with no required design fixes. Screenshots are deterministic actual-egui fixtures, not Logic captures or human/DAW interaction verification.

**Key Characteristics:**

- Graffiti identity with restrained native controls.
- Source and player visible together at the default size.
- Break identity reinforced by numbers and state outlines.

## Colors

### Primary

Lime accent signals active transport, focus, feedback, and selection outlines. Olive selection and hover fills differentiate native control states.

### Secondary

The four retained break colors identify sources consistently: orange, turquoise, periwinkle, and pink. Carry break numbers alongside color through selectors, waveforms, and placements.

### Neutral

Olive-black canvas surrounds the darker waveform wells. Surface fill and edge strokes organize controls; ivory carries primary text and muted sage carries supporting instructions.

**The Identity Rule.** Break colors identify audio sources; selection, locks, and playback also need labels, outlines, or symbols.

## Typography

Use egui's built-in proportional family for controls and instructions. Body and button roles share a size; small text is reserved for supporting labels. The Player title is bold. Painted waveform markers and placement details use a compact size (11 px); bar and placement labels use the small role. There is no custom UI font installation or web font stack.

The wordmark is a raster identity asset, not a display font. Preserve the approved lettering rather than rebuilding it as styled text.

## Layout

Default editor size is (1100 × 720 px), with a vertical scroll fallback and the inset token around content. Order: identity/global controls, compact source heading with selection, mute/bass tags and audition guidance, source waveform, source pads, player heading, integrated player, paired footer panels, and one feedback row. Source and player remain visible together.

Source pads divide the full width equally at the current slice count, using the slice-gap token, minimum width (26 px), and height (30 px). The player includes four bar-heading break dropdowns and a 16-beat ruler within its height token. Player slots divide the phrase evenly; source cuts remain sound-aligned and uneven.

The footer uses two equal-width functional panels: Placed slice and Phrase. Each has minimum inner height (118 px), footer inset, and footer radius. Empty, live, and frozen states replace placement controls with guidance. Feedback remains one row; export feedback includes its destination in the text and the full path in a tooltip. Native interaction size is (28 × 26 px), not a universal 28 px minimum height. The header has a 280 px Thin/Thicc crossfader, an 88 px 808 root selector, and a far-right 66 px Output rotary control with adjacent dB readout; the general slider width is (108 px). No mobile breakpoint system is implemented.

## Elevation & Depth

The main editor is flat: waveform wells, tonal fills, and strokes supply hierarchy with two functional footer panels and no decorative panels. Native egui popup/window defaults remain where not explicitly overridden; do not describe the implementation as globally shadow-free. Motion is functional transport repainting and immediate interaction feedback, with no ornamental animation.

## Shapes

Use the widget radius for native controls, the waveform radius for audio wells, and the placement radius for replacement regions. Normal control borders are (1 px); hover changes the border to lime, active controls use a lime border (2 px), and native selection uses a lime stroke (1.5 px). Widget expansion is zero.

## Components

- **Native buttons, selectors, and fields:** surface at rest, olive on hover/activation, ivory text, compact padding. Disabled controls use egui's disabled rendering and explanatory hover text. Keep native keyboard behavior.
- **Source waveform and pads:** selected waveform regions retain ivory outlines (2 px). Muted regions have black alpha (170); muted pad labels are struck through. Marker top handles make the cuts discoverable, and bass-hit dots identify tagged slices. Pads use opaque source fill with dark numbers and lime selection outlines (2 px). Clicking auditions, dragging places, and right-click or the visible Mute checkbox toggles mute. Marker dragging has horizontal grab tolerance (9 px).
- **Source patterns:** Break 1 orange is plain; Break 2 teal uses diagonals; Break 3 lavender uses dots; Break 4 pink uses horizontal lines. Repeat these patterns on pads and placed regions with subtle dark overlays, preserving readable numbers.
- **Player:** neutral gray ghost waveforms remain behind opaque source-colored replacements. Source-specific envelope previews are computed from the selected slice and account for Original/Fit timing, retriggers, and silent tails; they are not decorative copies of the full break. Placement labels are dark; narrow labels stack below width (48 px). Selection combines a dark inner border (3 px) and lime outer border (2 px). Locks have an ivory inner ring and dark padlock; empty selected slots retain a lime outline (2 px).
- **Transport and focus:** playing adds an active-bar header, subtle lime body tint (alpha 14), active beat text, and a white playhead (2 px) over a dark under-stroke (5 px). Stopped transport has no playhead. Player keyboard focus has a lime outline (2 px); arrows select slots and Enter places the chosen source slice. Mouse, keyboard, and controller selection share persistent state indicators.
- **Inspector and phrase actions:** a selected editable placement exposes length, speed, bit depth, retriggers, lock, and removal. Live and frozen states replace the inspector with guidance. Freeze and individual Lock remain distinct; Unfreeze and Unlock all slices provide separate recovery paths. Internal/MIDI region modes and export success/failure feedback remain explicit. The crossfader has Thin and Thicc endpoints and a Dry center mark; the far-right lower octave remains audible without an extra caption.
- **Wordmark:** the embedded PNG is decoded and uploaded on the UI thread at first successful use, then retained as a texture handle. Display is (250 × 65 px), with UV crop from (0, 0.15) to (1, 0.80). A text heading is the failure fallback; failed loads may retry. Keep decoding and visual work off the audio callback.

**The Host Gesture Rule.** Preserve parameter IDs and `PluginContext` paths: slider drags use `begin_edit` → `set_param` → `end_edit`; discrete keyboard/click edits use `automate`. Each UI frame synchronizes state and host-driven selections. Styling must not bypass host automation.

## Do's and Don'ts

- **Do** preserve the approved wordmark and the four numbered break identities.
- **Do** check ghost, selected, muted, locked, live, frozen, stopped, playing, dense, export, and missing-source states in actual egui renders.
- **Do** describe fixture screenshots separately from in-host verification.
- **Don't** add ornamental dashboard cards or decorative hardware panels.
- **Don't** conflate Freeze with placement locks or conceal disabled-action explanations.
- **Don't** treat HTML previews or an HTML detector as validation of this native Rust UI.

Output supports vertical dragging with host begin/set/end gestures, Shift fine adjustment, focused arrow-key adjustment, and double-click reset to 0 dB. Its lime arc and ivory pointer show the current host gain value.

The tone crossfader uses a 32 px interaction height, a 12 px recessed rail, an ivory ridged cap, and lime Dry center marks. Native slider keyboard behavior and host gestures remain intact; Double-click returns to Dry.

Header controls use equal gaps between playback status, crossfader, 808 root, and Output groups after the wordmark. The redundant Thin/Thicc heading and lower-octave caption are removed; the fader has no heading, duplicate readout, or Reset button; Thin, Dry, and Thicc are labeled once below the rail.
