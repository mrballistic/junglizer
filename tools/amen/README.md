# Built-in amen renderer

Builds the four one-bar amen loops in `sample/` (`174_JunglizerAmen_Bar_01.wav` to `_04.wav`) from Salamander Drumkit one-shots. Every sample comes from the kit, so nothing in the output derives from any reference recording.

**The renders in `sample/` are canonical.** This tool exists so they can be reproduced and audited, not so they can be replaced casually.

## Kit

Salamander Drumkit v1 (Alexander Holm, CC BY-SA 3.0), the overhead (`OH`) folder.

- Download: https://download.linuxaudio.org/musical-instrument-libraries/sfz/salamander_drumkit_v1.tar.7z
- Archive SHA-256: `a576ff79fcc33b53d8a373f459277e6727bd4e68d5383b9d69b1d9e0ec585329` (359,607,308 bytes; the download above matches)
- Unpack with `7z x salamander_drumkit_v1.tar.7z`, then untar. The one-shots used are in `OH/`.

## Setup

```bash
python3 -m venv venv && venv/bin/pip install numpy scipy
```

## Commands

```bash
python3 tools/amen/clone_amen.py render-final <kit>/OH <out dir>
python3 tools/amen/check_render.py <out dir> sample/
```

`render-final` reads only `transcription.json` (hit times with the closed-loop timing nudges baked in, instrument, velocity layer and level) and `render_settings.json` (choice `b`, seed 174, mix EQ, snare EQ, crash cut), plus the kit folder. `check_render.py` requires, per bar, an identical frame count (60828), every onset within 0.5 ms, and a residual at least 30 dB below the shipped level; it prints `4/4 bars match` and exits 0 on success. Against the pinned kit the render is bit-identical to `sample/`.

## The rule

Any new render that would replace `sample/` needs Todd’s listen first.

## History

The `transcribe`, `native`, `out3` and `out4` modes of `clone_amen.py` analysed reference audio that is not in the repo and never will be. They are kept as a record of how `transcription.json` and the settings were derived. `bake` replays the final timing loop from saved JSON (no audio) to produce the two committed files. `onsets.py` is the stdlib onset detector `check_render.py` uses.
