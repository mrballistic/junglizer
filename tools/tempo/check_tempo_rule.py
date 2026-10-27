#!/usr/bin/env python3
"""Check the beta 2 tempo rule against a folder of loops whose filenames carry a BPM.

Rule (design doc, "Length"): for each whole-bar beat count from MIN_BEATS to 32, BPM =
beats x 60 / duration; take the largest BPM at or below 195; accept it if it's 80 or above.

Read-only analysis. Usage: python3 tools/tempo/check_tempo_rule.py <folder> [<folder> ...]
WAV via the stdlib; AIFF/MP3 durations via macOS afinfo.
"""
import collections
import os
import re
import subprocess
import sys
import wave

MIN_BEATS, MAX_BEATS, LO, HI, TOL = 4, 32, 80.0, 195.0, 0.01


def duration(path):
    try:
        with wave.open(path) as w:
            return w.getnframes() / w.getframerate()
    except Exception:
        out = subprocess.run(["afinfo", path], capture_output=True, text=True).stdout
        m = re.search(r"estimated duration: ([\d.]+)", out)
        return float(m.group(1)) if m else None


def read(d):
    best = None
    for beats in range(MIN_BEATS, MAX_BEATS + 1, 4):
        bpm = beats * 60 / d
        if LO * (1 - TOL) <= bpm <= HI * (1 + TOL):
            best = (beats, bpm)
    return best


def named_bpm(name):
    m = re.search(r"(?<![\d.])(\d{2,3}(?:\.\d)?)(?=\s?bpm|_|\b)", name, re.I)
    v = float(m.group(1)) if m else None
    return v if v and 60 <= v <= 200 else None


def main():
    stats, rows = collections.Counter(), []
    for root in sys.argv[1:]:
        for dp, _, fs in os.walk(root):
            for f in sorted(fs):
                if not f.lower().endswith((".wav", ".aif", ".aiff", ".mp3")):
                    continue
                d = duration(os.path.join(dp, f))
                if not d:
                    stats["unreadable"] += 1
                    continue
                res, nb = read(d), named_bpm(f)
                if not res:
                    kind = "refused"
                elif not nb:
                    kind = "no bpm in name"
                elif abs(res[1] - nb) <= 1.5:
                    kind = "correct"
                elif abs(res[1] - 2 * nb) <= 3 or abs(res[1] - nb / 2) <= 1.5:
                    kind = "half/double fixes it"
                elif abs(res[1] - 1.5 * nb) <= 2.5 or abs(res[1] - nb / 1.5) <= 1.5:
                    kind = "off by 1.5x"
                else:
                    kind = "other mismatch"
                stats[kind] += 1
                if kind not in ("correct", "no bpm in name"):
                    rows.append((kind, f, round(d, 3), nb, res and (res[0], round(res[1], 1))))
    print(dict(stats))
    for r in rows:
        print(r)


if __name__ == "__main__":
    main()
