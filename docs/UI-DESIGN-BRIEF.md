# Junglizer UI design brief

## Assignment

Take one thoughtful design and implementation pass at Junglizer's plugin editor. Make the source-to-player workflow immediately understandable and the controls comfortable to use while a producer listens in Logic. Preserve the working audio engine, project state, MIDI export, and host automation. Deliver a coherent visual system, an implemented UI, and screenshots of the actual editor in its important states.

This is a desktop audio instrument, initially an AUv2 in Logic Pro; the same Rust editor is packaged as VST3 and CLAP. The audience is a producer auditioning and rearranging breaks during playback, often moving quickly between mouse, keyboard, and a CS12 controller. The UI should feel rhythmically alive and precise, with the clarity of a good sampler rather than decorative hardware imitation.

## Product model the UI must teach

- Four supplied **one-bar, four-beat** breaks at 174 BPM feed a **four-bar, 16-beat** phrase. Each of the four bars chooses one break. Host tempo sync currently changes playback speed and pitch.
- **Source slices:** one shared count from 1–32 applies to all four source breaks. Each break has its own editable, sound-aligned onset markers for each count. A slice may be quiet or muted; marker locations are deliberately uneven.
- **Player:** 1–32 *evenly spaced* slots span the entire four-bar phrase. The selected full breaks form a visible and audible **ghost** underneath the player. An empty slot still plays its ghost audio. Drag or click a source slice into a slot to replace the ghost for that span.
- A placed slice has length in slots, original speed or pitch-changing fit-to-length, 1–16-bit depth, 1/2/4/8 retriggers, and a lock. Adjacent placements must remain distinct; a failed length extension should be explained clearly.
- **Suggest a fill** edits one selected bar and preserves locked slices. **Live variations** make a new phrase every four bars. **Freeze** captures the exact audible phrase and bar choices for repeat playback and MIDI export. Frozen and live states limit editing.
- **Thin ↔ Thicc** affects the entire player. Center is dry; left increasingly high-passes; right adds a root-tuned 808 on bass-tagged source hits; far right adds a lower octave. Bass-hit tags can be corrected per source slice.
- During host playback, show a moving playhead across all 16 beats and make the active bar unmistakable. Stopped playback must look stopped.
- **Export MIDI** saves a four-bar `.mid` file under `~/Music/Junglizer/Exports` and reveals it in Finder. The producer drags it onto Junglizer's Logic track, then selects **MIDI region** playback. Direct drag from the plugin is not implemented.
- The plugin exposes 29 Logic automation parameters for live CS12 control. UI edits to host parameters must keep Logic automation and MIDI assignments working.

## Primary task and recommended hierarchy

The main action is: **find a sound in a source break, place it in the four-bar player, hear the change, then refine or freeze it**. Source and player should be visible together at the default window size so dragging feels direct. Give the player the strongest visual weight; its ghost, placed regions, bar boundaries, slot grid, playhead, and selected edit should remain legible at 32 slots.

Suggested organization:

1. A compact header with identity, playback status, output, and Thin ↔ Thicc. Keep global sound shaping reachable without pushing the timeline below the fold.
2. Source area with four clearly identified break choices, shared slice-count control, waveform and editable markers, slice selection/audition, mute, and bass-hit tagging.
3. Four-bar player as the focal canvas. Integrate each bar's break choice with that bar if it improves comprehension. Show ghost waveform behind replacements and use a distinct selected-placement treatment.
4. A contextual inspector for the selected placement. Put length, speed, bit depth, retriggers, lock, and remove together rather than scattering them across the page.
5. A compact variation/export area for suggest, live, freeze/unfreeze, playback mode, and Finder export. State-dependent actions should explain why they are unavailable.

Do not use generic dashboard cards or large decorative panels. Avoid forcing a long scroll between source and player. If vertical space is tight, use progressive disclosure for advanced editing while leaving the two waveforms visible.

## Visual and widget system

The incumbent is dark and functional, but its controls mix hand-painted canvas elements with default egui widgets. Treat its colors as a reference, not an immutable brand: background `#19232B`; text `#DBE7EE`; muted text `#798F9D`; accent `#F5AE4B`; break colors `#EE9757`, `#69CDC1`, `#97ADF1`, `#DE8BBE`. Source waveform is 112 px high, player is 150 px high, and both are painted directly in `src/ui.rs`. Waveform peaks are computed in code at 512 bins per source; no PNG preparation is required.

Define a small, consistent set of design tokens before changing widgets: canvas/surface colors, text levels, source colors, selected/hover/focus/disabled/locked states, type sizes, spacing, radii, stroke widths, and a minimum usable click target. Use color to identify the four breaks, but pair it with break numbers and patterns or outlines where state matters. The playhead should read over every break color and over placed slices. Use a legible UI sans; avoid tiny all-caps copy for instructions or values.

Technical styling detail: this is **egui 0.34** inside **truce-egui 6.3**. The current editor only sets `ui.visuals_mut().dark_mode = true` and item spacing `(9, 8)` inside a vertical `ScrollArea`; most buttons, sliders, checkboxes, combo boxes, and text keep stock styling. `truce_egui::widgets::param_knob` has a fixed **60 × 82 px** cell with **10 px** value and label text; it reads several colors from `ui.visuals().widgets`, but hardcodes its hover gray. `param_dropdown` uses **80 px per column × 82 px** and a **10 px** label. A palette change through `egui::Visuals` will only partly restyle them. If replacing either widget, preserve the `PluginContext` parameter gesture path (`begin_edit`, `set_param`, `end_edit`, or `automate`) and its host-visible parameter ID. Avoid a visual mismatch between Truce widgets and custom controls.

Keep animation purposeful and light: host playhead motion, hover/selection feedback, and a clear freeze transition are useful; animated decoration is not. Do not allocate files, decode samples, or do visual work on the audio callback.

## States and interaction details

Design and check at least these states: first open with only ghost audio; selected source slice; selected placement and inspector; quiet/muted source slice; 32 source slices and 32 player slots; locked placement; no available space for a length change; stopped versus playing with active bar; live variation before and after a new four-bar pass; frozen phrase; internal versus MIDI-region playback; successful and failed MIDI export; missing local WAVs. Show disabled actions as intentional with short explanations. Keep hover, drag, keyboard focus, selection, and controller-driven changes visually consistent.

Preserve the current interactions unless you replace them with a clearer equivalent: click source tile to audition, drag it to the player, click an empty player slot to place, click a placement to inspect, drag waveform markers, right-click a source tile to mute/unmute, and use the bass-hit checkbox for the selected slice. The current editor also has bar-source dropdowns, a fill-bar selector, live/freeze buttons, and the MIDI playback switch. Tooltip or inline hints should teach shortcuts without making right-click the only discoverable way to mute.

Use concise musical language. Distinguish **source slice count** from **player slot count**. Keep the unit of length visible (“slots”). Say what **Fit to length** does to pitch. Make **Thin**, **Dry**, **Thicc**, and **Extra Thicc** perceptually clear around the center point. MIDI export feedback should state where the file went and the next Logic step.

## Implementation map and boundaries

- Start with [`src/ui.rs`](../src/ui.rs): all layout, painted waveforms, markers, player, playhead, inspector, messages, and UI interactions live there.
- [`src/lib.rs`](../src/lib.rs) declares the 29 host parameters, exposes transport through `PluginContext`, and creates the editor at **1100 × 720 px**. Keep parameter names/IDs and host automation behavior stable unless a change is deliberately migrated.
- [`src/edit_state.rs`](../src/edit_state.rs) stores marker sets, placements, live/frozen state, and serialization. [`src/live.rs`](../src/live.rs) supplies the displayed live phrase. [`src/sample.rs`](../src/sample.rs) produces onset suggestions and waveform source data. The design pass should avoid DSP or state-format changes unless a specific UI issue requires one.
- The shipped samples are local and git-ignored. Do not add them to source control, generate image assets from them, or assume a clone contains them.
- There is no committed product/design system document or screenshot baseline yet. Capture the incumbent editor in Logic or the standalone host before committing to a visual direction; document any design tokens you introduce.

## Acceptance check

At 1100 × 720 in the real plugin window, a new user can tell where sounds come from, where they play, which bar is active, what is selected, and whether the phrase is live or frozen without reading a paragraph. Source-to-player dragging is obvious and remains usable at 32 slots. Controls remain mapped to Logic automation; mouse edits and controller edits display the same state. Verify the first-open, populated, maximum-density, playing, and frozen/MIDI states with screenshots. Run `cargo test`, build the AU, and run Apple's AU validation; confirm VST3 and CLAP still build. Report any UI behavior you could not exercise inside Logic.
