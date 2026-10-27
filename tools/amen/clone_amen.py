#!/usr/bin/env python3
"""SPIKE (throwaway): clone four one-bar amen cuts hit for hit on the Salamander
Drumkit (CC BY-SA 3.0, Alexander Holm).

The reference cuts are analysed only: hit times, which instrument, and relative
level. No audio from them reaches the output. Every output sample comes from
Salamander one-shots.

Needs numpy + scipy (use the throwaway venv in target/amen-spike/venv).

  clone_amen.py transcribe <ref dir> <kit OH dir> <transcription.json>
  clone_amen.py render     <transcription.json> <kit OH dir> <out dir>
  clone_amen.py verify     <transcription.json> <out dir> <kit OH dir>
  clone_amen.py all        <ref dir> <kit OH dir> <work dir>     (all three, with
                           closed-loop timing correction; writes <work>/out2)
  clone_amen.py out3       <ref dir> <kit OH dir> <work dir>     (tone EQ, bar 4 tom,
                           bar 3 crash cut; writes <work>/out3 and a local-only A/B)
  clone_amen.py out4       <ref dir> <kit OH dir> <work dir>     (lean low end; bar 4
                           as loTom (a) or kick (b); writes <work>/out4)
  clone_amen.py bake       <work dir> <kit OH dir> <out dir>   (replays out4's timing loop from
                           saved JSON only, bakes the nudges in: writes <out>/transcription.json
                           and <out>/render_settings.json)
  clone_amen.py render-final <kit OH dir> <out dir>   (reads tools/amen/transcription.json and
                           render_settings.json only; writes the four shipped bars)
  clone_amen.py native     <originals dir> <kit OH dir> <work dir>  (analyse KAN's
                           un-stretched cuts; writes <work>/transcription_native.json)
"""
import glob
import json
import os
import sys
import wave

import numpy as np
import scipy.io.wavfile as wf
import scipy.optimize as so
import scipy.signal as ss

BPM = 174.0
STEP = 60.0 / BPM / 4
SR = 44100
KIT_SR = 48000
BAR = 60828                          # one bar, frames at 44.1 kHz (reference format)
REF_NAMES = ["174_AmenBreak_Cut_01_NoStab", "174_AmenBreak_Cut_02",
             "174_AmenBreak_Cut_03", "174_AmenBreak_Cut_04"]
OUT_NAME = "174_JunglizerAmen_Bar_{:02d}.wav"

# Analysis bands: (low Hz, high Hz, future window s, past window s, min rise dB)
BANDS = {
    "kick":  (None, 150, 0.008, 0.030, 5.0),
    "body":  (150, 1000, 0.005, 0.025, 4.0),
    "crack": (2000, 6000, 0.003, 0.020, 4.0),
    "cym":   (7000, None, 0.003, 0.020, 4.0),
}
INSTS = ["kick", "snare", "ride"]          # separated by NNLS; crash by its wash
# Onset bands that can time each instrument, in order of preference.
TIME_BANDS = {"tom": ["body", "kick"], "lotom": ["body", "kick"], "kick": ["kick", "body"], "snare": ["crack", "body"], "ride": ["cym"],
              "crash": ["cym", "crack"]}
# Kit files: instrument -> {layer: glob}
KIT = {
    "kick":  {"FF": "kick_OH_FF_*", "F": "kick_OH_F_*", "P": "kick_OH_P_*"},
    "snare": {"FF": "snare_OH_FF_*", "F": "snare_OH_F_*", "MP": "snare_OH_MP_*",
              "Ghost": "snare_OH_Ghost_*"},
    "ride":  {"FF": "ride1_OH_FF_*", "MP": "ride1_OH_MP_*"},
    "crash": {"FF": "crash1_OH_FF_*", "P": "crash1_OH_P_*"},
    "tom":   {"FF": "hiTom_OH_FF_*", "F": "hiTom_OH_F_*", "P": "hiTom_OH_P_*"},  # out3 only
    "lotom": {"FF": "loTom_OH_FF_*", "MP": "loTom_OH_MP_*", "PP": "loTom_OH_PP_*"},  # out4a
}
# Velocity classes: (min level dB re that instrument's loudest hit, class, kit layer)
VEL = {
    "kick":  [(-4, "accent", "FF"), (-12, "normal", "F"), (-99, "soft", "P")],
    "snare": [(-3, "accent", "FF"), (-9, "normal", "F"), (-16, "soft", "MP"),
              (-99, "ghost", "Ghost")],
    "ride":  [(-3, "accent", "FF"), (-99, "normal", "MP")],
    "crash": [(-6, "accent", "FF"), (-99, "normal", "P")],
}
GATE = {"kick": 22, "snare": 28, "ride": 18}  # dB below the instrument's loudest hit
MERGE = {"kick": 0.030, "snare": 0.030, "ride": 0.015, "crash": 0.600}
WASH_DB = 3.0                        # 2-6 kHz floor this far over the median = crash
NB = np.geomspace(30, 18000, 41)     # spectral band edges for the observations
_MID = np.sqrt(NB[:-1] * NB[1:])
FEAT = np.stack([_MID < 150, (_MID >= 150) & (_MID < 1000), (_MID >= 2000) & (_MID < 6000),
                 _MID >= 7000]).astype(float)        # K, B, C, Y
# NNLS weight per K, B, C, Y band. The reference kick blooms through the body band
# unevenly from hit to hit, so there the body band is left out and kick vs snare is
# told by <150 Hz vs crack; Salamander's overhead snare has little crack, so for the
# kit the body band is what tells them apart. (Part of each kit's calibration.)
# The reference has one signature per instrument, so its bands are simply equalised;
# the kit has one per dynamic layer and its crack band is tiny for every instrument,
# so equalising lets that band dominate: refine the fit in dB instead.
REF_W = (np.array([1.0, 0.0, 1.0, 1.0]), "equal")
KIT_W = (np.array([1.0, 1.0, 1.0, 1.0]), "log")
POST = (-0.0005, 0.020)              # observation windows around an onset (s)
PRE = (-0.021, -0.0005)


# ---------------------------------------------------------------- audio io

def load(path):
    sr, x = wf.read(path)
    if x.dtype == np.int32:
        x = x.astype(np.float64) / 2 ** 31
    elif x.dtype == np.int16:
        x = x.astype(np.float64) / 2 ** 15
    else:
        x = x.astype(np.float64)
    if x.ndim == 1:
        x = np.stack([x, x], 1)
    return sr, x


def write24(path, x):
    x = np.clip(x, -1.0, 1.0 - 1.0 / 8388608)
    q = np.round(x * 8388608).astype("<i4")
    b = q.reshape(-1, 1).view(np.uint8).reshape(-1, 4)[:, :3].tobytes()
    with wave.open(path, "wb") as w:
        w.setnchannels(x.shape[1])
        w.setsampwidth(3)
        w.setframerate(SR)
        w.writeframes(b)


def to44(x):
    return ss.resample_poly(x, 147, 160, axis=0)


# ---------------------------------------------------------------- detection

def _sos(lo, hi):
    if lo is None:
        return ss.butter(4, hi, "lowpass", fs=SR, output="sos")
    if hi is None:
        return ss.butter(4, lo, "highpass", fs=SR, output="sos")
    return ss.butter(4, [lo, hi], "bandpass", fs=SR, output="sos")


SOS = {b: _sos(v[0], v[1]) for b, v in BANDS.items()}


def band_odf(m, band):
    """Energy ratio (dB) of the next few ms over the previous ~25 ms, per sample."""
    lo, hi, fw, pw, _ = BANDS[band]
    pad = int(0.05 * SR)
    y = ss.sosfilt(SOS[band], np.concatenate([np.zeros(pad), m, np.zeros(pad)]))
    p = y * y
    c = np.concatenate([[0.0], np.cumsum(p)])
    n = len(p)
    i = np.arange(n)
    F, P, g = int(fw * SR), int(pw * SR), int(0.001 * SR)
    fut = (c[np.minimum(i + F, n)] - c[i]) / F
    past = (c[np.maximum(i - g, 0)] - c[np.maximum(i - g - P, 0)]) / P
    floor = fut.max() * 1e-5                      # -50 dB re band peak: silence floor
    r = 10 * np.log10((fut + floor) / (past + floor))
    lvl = 10 * np.log10(fut + 1e-20)
    return r[pad:pad + len(m)], lvl[pad:pad + len(m)]


def band_onsets(m, band):
    r, lvl = band_odf(m, band)
    top = lvl.max()
    pk, _ = ss.find_peaks(r, height=BANDS[band][4], prominence=3.0, distance=int(0.012 * SR))
    out = []
    for p in pk:
        if lvl[p] < top - 40:
            continue
        # sub-sample peak by parabolic interpolation
        if 0 < p < len(r) - 1:
            a, b, cc = r[p - 1], r[p], r[p + 1]
            den = a - 2 * b + cc
            d = 0.5 * (a - cc) / den if den != 0 else 0.0
        else:
            d = 0.0
        out.append(((p + d) / SR, float(r[p]), float(lvl[p] - top)))
    return out


def band_spec(m, t0, t1):
    i0, i1 = max(0, int(round(t0 * SR))), min(len(m), int(round(t1 * SR)))
    if i1 - i0 < 16:
        return np.zeros(len(NB) - 1)
    seg = m[i0:i1] * ss.windows.tukey(i1 - i0, 0.1)
    X = np.abs(np.fft.rfft(seg, 2048)) ** 2 / (i1 - i0)
    f = np.fft.rfftfreq(2048, 1 / SR)
    return np.array([X[(f >= NB[k]) & (f < NB[k + 1])].sum() for k in range(len(NB) - 1)])


def observation(m, t):
    post = band_spec(m, t + POST[0], t + POST[1])
    pre = band_spec(m, t + PRE[0], t + PRE[1]) if t + PRE[0] >= 0 else 0.0
    return np.maximum(post - pre, 0.0)


def events(m):
    """Cluster band onsets into events. Returns [(t_event, {band: (t, rise, lvl)})]."""
    cands = []
    for b in BANDS:
        for t, r, l in band_onsets(m, b):
            cands.append((t, b, r, l))
    # The cut starts on a hit: treat the first nonsilent sample as an onset in all bands.
    first = int(np.argmax(np.abs(m) > 1e-3 * np.abs(m).max()))
    if first < int(0.002 * SR):
        cands = [c for c in cands if c[0] > 0.012]
        for b in BANDS:
            cands.append((first / SR, b, 99.0, 0.0))
    cands.sort()
    evs = []
    for c in cands:
        if evs and c[0] - evs[-1][0] < 0.010 and c[1] not in evs[-1][1]:
            evs[-1][1][c[1]] = c[0:1] + c[2:]
        else:
            evs.append((c[0], {c[1]: c[0:1] + c[2:]}))
    return evs


# ---------------------------------------------------------------- kit

KIT_NAMES = {"ride1": "ride", "crash1": "crash", "hiTom": "tom", "loTom": "lotom"}


class Kit:
    def __init__(self, oh, eq=None, snare_eq=None):
        """eq / snare_eq: the out3 tone filters, so the verifier is calibrated on the
        kit as it is heard in the mix (the analysis of an EQ'd hit must use EQ'd
        signatures)."""
        self.files = {}       # (inst, layer) -> [path]
        self.cache = {}
        self.eq, self.snare_eq = eq, snare_eq
        for inst, layers in KIT.items():
            for layer, pat in layers.items():
                fs = sorted(glob.glob(os.path.join(oh, pat + ".wav")),
                            key=lambda p: int(p.rsplit("_", 1)[1][:-4]))
                self.files[(inst, layer)] = [p for p in fs if self.sample(p) is not None]

    def sample(self, path):
        """(48 kHz stereo audio, detected onset s, observation spectrum) or None."""
        if path in self.cache:
            return self.cache[path]
        sr, x = load(path)
        assert sr == KIT_SR
        a = np.abs(x).max(1)
        start = int(np.argmax(a > 0.05 * a.max()))
        if start > 0.05 * sr:            # e.g. kick_OH_P_1: 1.2 s of noise first
            self.cache[path] = None
            return None
        x = x[:int(4.0 * sr)]
        inst = os.path.basename(path).split("_")[0]
        inst = KIT_NAMES.get(inst, inst)
        if inst == "snare" and self.snare_eq:
            x = ss.sosfilt(eq_sos(self.snare_eq), x, axis=0)
        if self.eq:
            x = ss.sosfilt(eq_sos(self.eq), x, axis=0)
        m = to44(x).mean(1)
        pad = int(0.05 * SR)
        mm = np.concatenate([np.zeros(pad), m])
        t = None
        for b in TIME_BANDS[inst]:
            ons = band_onsets(mm, b)
            if ons:
                t = ons[0][0]
                break
        t = (t if t is not None else pad / SR + start / sr) - pad / SR
        # observe it the way a hit in a mix is observed: from its first band onset
        ev = events(mm)
        spec = observation(mm, ev[0][0] if ev else t + pad / SR)
        # where the attack really begins (-40 dB re peak): a hit on a bar line starts here
        attack = int(np.argmax(a > 0.01 * a.max())) / sr
        floor = floor_power(ss.sosfilt(_sos(2000, 6000), m), t) if inst == "crash" else None
        self.cache[path] = (x, t, spec, attack, floor)
        return self.cache[path]

    def templates(self):
        """One 4-band signature per instrument and dynamic layer (the layers differ:
        a ghost snare has relatively more low end than a full one)."""
        cols, labels = [], []
        for inst in INSTS:
            for layer in KIT[inst]:
                f = np.array([FEAT @ self.sample(p)[2] for p in self.files[(inst, layer)]])
                cols.append(np.median(f / f.sum(1, keepdims=True), 0))
                labels.append(inst)
        return np.stack(cols, 1), labels

    def mean_templates(self):
        T, labels = self.templates()
        return np.stack([T[:, [l == i for l in labels]].mean(1) for i in INSTS], 1)


# ---------------------------------------------------------------- transcription

def db(v):
    return 10 * np.log10(np.maximum(v, 1e-20))


def bar_events(bars, before=None):
    """Every onset cluster in every bar, with its added-energy observation.
    before: optional audio that really preceded each bar (else the previous bar)."""
    evs = []
    for bi, m in enumerate(bars):
        for te, bands in events(m):
            if te < 0.002:
                # the cut starts on this hit; what rang before it is the previous bar's end
                prev = before[bi] if before is not None else bars[bi - 1]
                pre = band_spec(prev, len(prev) / SR + PRE[0] - PRE[1], len(prev) / SR)
                obs = np.maximum(band_spec(m, 0, POST[1] - POST[0]) - pre, 0.0)
            else:
                obs = observation(m, te)
            evs.append({"bar": bi, "t": te, "bands": bands, "obs": obs, "f": FEAT @ obs})
    return evs


def exemplar_templates(evs, fallback, ride_cy_max=-2):
    """Per-instrument 4-band signature (K, B, C, Y) of this recording, taken from hits
    that only one instrument can explain; falls back to the kit's signature."""
    rules = {
        "kick": lambda d, b: "kick" in b and d[0] - d[2] >= 12 and d[1] - d[2] >= 15,
        "snare": lambda d, b: "crack" in b and "body" in b and 5 <= d[1] - d[2] <= 14
        and d[2] - d[3] >= 0 and d[0] - d[2] <= -5,
        # (the un-stretched originals' ride is crackier: there ride_cy_max is None)
        "ride": lambda d, b: "cym" in b and (ride_cy_max is None or d[2] - d[3] <= ride_cy_max)
        and d[1] - d[2] <= -3 and d[0] - d[2] <= -10,
    }
    cols, used = [], {}
    for k, inst in enumerate(INSTS):
        ex = [e["f"] / e["f"].sum() for e in evs if rules[inst](db(e["f"]), e["bands"])]
        used[inst] = len(ex)
        cols.append(np.median(ex, 0) if len(ex) >= 3 else fallback[:, k])
    return (np.stack(cols, 1), list(INSTS)), used


def floor_power(y, t):
    """Lowest 20 ms power of a 2-6 kHz signal 60-250 ms after t (0 if too short)."""
    i0, i1 = int((t + 0.06) * SR), min(len(y), int((t + 0.25) * SR))
    if i1 - i0 < int(0.08 * SR):
        return 0.0
    return float(np.min([np.mean(y[i:i + 882] ** 2) for i in range(i0, i1 - 882, 441)]))


def wash_db(y, t, bg):
    """Floor of the 2-6 kHz level 60-250 ms after t, over the median level (dB).
    A ride (or a snare) decays back to the background between strokes; a crash's
    wash keeps the floor up."""
    fp = floor_power(y, t)
    return float(db(fp) - bg) if fp > 0 else -99.0


def separate(f, tmpl, bw):
    """Split an event's added energy (K, B, C, Y) into kick, snare and ride activations.

    NNLS over the instrument signatures, each band scaled by the signatures' mean level
    there. bw = (band weights, mode); mode "log" then refines the fit in dB. Also
    returns each instrument's share of the model in the band where its signature
    stands out most from the others."""
    T, labels = tmpl
    bw, mode = bw
    w = bw / T.mean(1)
    a, _ = so.nnls(T * w[:, None], f * w)
    if mode == "log":
        # refine in dB with a 30 dB floor: an empty band counts against an instrument
        # that would put energy there, a quiet band still counts for its presence
        eps = 1e-3 * f.max() + 1e-30
        on = bw > 0
        def res(x):
            return (db(T[on] @ x + eps) - db(f[on] + eps)) * bw[on]
        x0 = np.maximum(a, 1e-4 * f.max() + 1e-30)
        a = so.least_squares(res, x0, bounds=(0, np.inf), x_scale=x0 + f.max() * 1e-2).x
        a[a < 1e-3 * a.max()] = 0.0
    P = T * a
    total = P.sum(1) + 1e-30
    mean = np.stack([T[:, [l == i for l in labels]].mean(1) for i in INSTS], 1)
    rel = mean / mean.sum(1, keepdims=True) * (bw > 0)[:, None]
    act, share = np.zeros(len(INSTS)), {}
    for k, inst in enumerate(INSTS):
        sel = [l == inst for l in labels]
        act[k] = a[sel].sum()
        b = int(np.argmax(rel[:, k]))
        share[inst] = float(P[b, sel].sum() / total[b])
    return act, share


SHARE = {"kick": 0.6, "snare": 0.5, "ride": 0.5}
KICK_UNDER_SNARE = 12.0
KICK_SNARE_DB = 4.0


def classify(bars, evs, T, bw, merge_keep="first"):
    y = [ss.sosfilt(_sos(2000, 6000), m) for m in bars]
    seg = np.concatenate([[np.mean(v[i:i + 882] ** 2) for i in range(0, len(v) - 882, 441)] for v in y])
    bg = float(db(np.median(seg)))
    hits = []
    for e in evs:
        a, share = separate(e["f"], T, bw)
        wash = wash_db(y[e["bar"]], e["t"], bg)
        e["wash"] = wash
        crash = (wash >= WASH_DB and ("cym" in e["bands"] or "crack" in e["bands"])
                 and not any(h["inst"] == "crash" and h["bar"] == e["bar"]
                             and e["t"] - h["t"] < MERGE["crash"] for h in hits))
        if crash:
            tb = "cym" if "cym" in e["bands"] else "crack"
            hits.append({"bar": e["bar"], "t": e["bands"][tb][0], "inst": "crash",
                         "act": float(db(e["f"][2] + e["f"][3])), "wash": wash,
                         "floor": float(db(floor_power(y[e["bar"]], e["t"]) - 10 ** (bg / 10))),
                         "rise": e["bands"][tb][1]})
        for k, inst in enumerate(INSTS):
            if a[k] <= 0 or share[inst] < SHARE[inst]:
                continue
            if crash and inst != "kick":
                continue                 # the crash explains this event's crack and air
            tb = next((b for b in TIME_BANDS[inst] if b in e["bands"]), None)
            if tb is None:
                continue
            if inst == "snare" and "body" not in e["bands"]:
                continue                 # crack with no body is cymbal, not a snare
            if inst == "snare" and tb == "body" and (
                    ("kick" in e["bands"] and db(a[0]) > db(a[1]) - 6) or e["bands"]["body"][1] < 8):
                continue                 # body-only snare: only a clear ghost, well over any kick
            if inst in ("kick", "snare") and a[0] > 0 and a[1] > 0 and \
                    db(a[k]) < db(a[1 - k]) - KICK_SNARE_DB:
                continue                 # one drum's spill into the other's bands
            hits.append({"bar": e["bar"], "t": e["bands"][tb][0], "inst": inst,
                         "act": float(db(a[k])), "wash": wash, "ev": id(e),
                         "rise": e["bands"][tb][1]})
    top = {}
    for h in hits:
        top[h["inst"]] = max(top.get(h["inst"], -999), h["act"])
    out = []
    for bi in range(len(bars)):
        hb = sorted([h for h in hits if h["bar"] == bi], key=lambda h: h["t"])
        keep = []
        for h in hb:
            rel = h["act"] - top[h["inst"]]
            if h["inst"] != "crash" and rel < -GATE[h["inst"]]:
                continue
            if h["inst"] == "kick" and rel < -KICK_UNDER_SNARE and any(
                    o["inst"] == "snare" and 0 < h["t"] - o["t"] < 0.030 and o.get("ev") != h.get("ev")
                    for o in hb):
                continue                 # quiet lows just after a snare are the snare's own
            prev = next((d for d in reversed(keep) if d["inst"] == h["inst"]), None)
            if prev and h["t"] - prev["t"] < MERGE[h["inst"]]:
                # a second stage of the same stroke (the reference's kicks bloom over
                # ~20 ms and its snares have a second burst): keep the first onset time
                # (merge_keep="rise": keep the sharper onset's time; the native cuts have
                # weak precursors instead of blooms)
                if merge_keep == "rise" and h.get("rise", 0) > prev.get("rise", 0):
                    prev["t"], prev["rise"] = h["t"], h["rise"]
                prev["act"] = float(db(10 ** (prev["act"] / 10) + 10 ** (h["act"] / 10)))
                continue
            keep.append(dict(h))
        for h in keep:
            # levels relative to the instrument's loudest hit across all four bars
            h["rel"] = h["act"] - top[h["inst"]]
        out.append(keep)
    # recompute relative levels after merging
    top2 = {}
    for b in out:
        for h in b:
            top2[h["inst"]] = max(top2.get(h["inst"], -999), h["act"])
    for b in out:
        for h in b:
            h["rel"] = round(h["act"] - top2[h["inst"]], 2)
            h["vel"], h["layer"] = next((v, l) for th, v, l in VEL[h["inst"]] if h["rel"] >= th)
    return out, bg


def analyse(paths, kit=None, templates=None, bw=REF_W, ride_cy_max=-2, before=None,
            merge_keep="first"):
    bars = []
    for p in paths:
        sr, x = load(p)
        assert sr == SR
        bars.append(x.mean(1))
    evs = bar_events(bars, before)
    if templates is None:
        T, used = exemplar_templates(evs, kit.mean_templates(), ride_cy_max)
    else:
        T, used = templates, None
    res, bg = classify(bars, evs, T, bw, merge_keep)
    return res, T, used, bars


def transcribe(ref_dir, oh, out_json):
    kit = Kit(oh)
    paths = [os.path.join(ref_dir, n + ".wav") for n in REF_NAMES]
    bars_hits, T, used, bars = analyse(paths, kit)
    rows = []
    for bi, b in enumerate(bars_hits):
        for h in b:
            rows.append({"bar": bi + 1, "time": round(h["t"], 6), "step": round(h["t"] / STEP, 3),
                         "instrument": h["inst"], "velocity": h["vel"], "layer": h["layer"],
                         "level_db": h["rel"], "activation_db": round(h["act"], 2),
                         **({"wash_floor_db": round(h["floor"], 2)} if "floor" in h else {})})
    chokes = find_chokes(bars, rows)
    with open(out_json, "w") as f:
        json.dump({"bpm": BPM, "step_s": STEP,
                   "source": "timing/level analysis of the four reference cuts (no audio)",
                   "reference_signatures_KBCY": T[0].T.round(5).tolist(), "exemplars": used,
                   "hits": rows, "chokes": chokes}, f, indent=1)
    print("reference signatures from", used, "exemplar hits")
    print_table(rows, chokes)
    return rows


def find_chokes(bars, rows):
    """Where a crash's 2-6 kHz wash falls >8 dB inside 15 ms and stays down for the
    next 50 ms, away from any hit: the cymbal was choked (or the cut was edited)."""
    out = []
    for bi, m in enumerate(bars):
        crashes = [r for r in rows if r["bar"] == bi + 1 and r["instrument"] == "crash"]
        if not crashes:
            continue
        hits = sorted(r["time"] for r in rows if r["bar"] == bi + 1)
        y = ss.sosfilt(_sos(2000, 6000), m)
        w = int(0.004 * SR)
        e = db(np.convolve(y * y, np.ones(w) / w)[:len(y)])
        lag, hold = int(0.015 * SR), int(0.050 * SR)
        for c in crashes:
            j = np.arange(int((c["time"] + 0.10) * SR), len(m) - lag - hold, 32)
            if len(j) == 0:
                continue
            def held(k):          # stays down until the next hit (15-50 ms)
                nxt = min([h for h in hits if h > k / SR] + [len(m) / SR])
                n = int(min(hold, (nxt - k / SR - 0.002) * SR - lag))
                return e[k + lag:k + lag + n].max() if n >= int(0.015 * SR) else e[k]
            after = np.array([held(k) for k in j])
            drop = e[j] - after
            ok = [k for k in np.argsort(-drop)[:80]
                  if not any(-0.010 < h - j[k] / SR < 0.030 for h in hits)]
            if ok and drop[ok[0]] > 8:
                k = ok[0]
                # the choke starts where the level begins to fall
                seg = e[j[k]:j[k] + lag]
                t = (j[k] + int(np.argmax(seg < e[j[k]] - 3))) / SR
                out.append({"bar": bi + 1, "crash_time": c["time"], "choke_time": round(t, 6),
                            "step": round(t / STEP, 3), "drop_db": round(float(drop[k]), 1)})
    return out


def print_table(rows, chokes=()):
    for bi in range(1, 5):
        b = [r for r in rows if r["bar"] == bi]
        cnt = {}
        for r in b:
            cnt[r["instrument"]] = cnt.get(r["instrument"], 0) + 1
        print(f"bar {bi}: " + ", ".join(f"{k} {v}" for k, v in sorted(cnt.items())))
        for r in b:
            print(f"   {r['time'] * 1000:8.2f} ms  step {r['step']:6.2f}  {r['instrument']:5s} "
                  f"{r['velocity']:6s} {r['level_db']:6.1f} dB")
        for c in chokes:
            if c["bar"] == bi:
                print(f"   {c['choke_time'] * 1000:8.2f} ms  step {c['step']:6.2f}  crash choke "
                      f"(-{c['drop_db']} dB)")


# ---------------------------------------------------------------- render

def biquad(kind, fc, gain_db, q, fs=KIT_SR):
    """RBJ cookbook peaking / low shelf / high shelf, as an sos row."""
    A = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * fc / fs
    al = np.sin(w0) / (2 * q)
    c = np.cos(w0)
    if kind in ("highpass", "lowpass"):
        # 2nd-order Butterworth: 12 dB/octave
        return ss.butter(2, fc, "high" if kind == "highpass" else "low", fs=fs, output="sos")[0]
    if kind == "peak":
        b = [1 + al * A, -2 * c, 1 - al * A]
        a = [1 + al / A, -2 * c, 1 - al / A]
    else:
        sq = 2 * np.sqrt(A) * al
        sgn = 1 if kind == "lowshelf" else -1
        b = [A * ((A + 1) - sgn * (A - 1) * c + sq), sgn * 2 * A * ((A - 1) - sgn * (A + 1) * c),
             A * ((A + 1) - sgn * (A - 1) * c - sq)]
        a = [(A + 1) + sgn * (A - 1) * c + sq, -sgn * 2 * ((A - 1) + sgn * (A + 1) * c),
             (A + 1) + sgn * (A - 1) * c - sq]
    return np.array(b + a) / a[0]


def eq_sos(filters, fs=KIT_SR):
    return np.stack([biquad(k, f, g, q, fs) for k, f, g, q in filters])


def render(js, oh, out_dir, nudges=None, seed=174, kit=None, opts=None):
    """opts (out3): eq = [(kind, fc, dB, Q)] on the mix; snare_eq = same, per snare hit;
    tom = {"bar", "step", "layer"}: play that hit on the tom; crash_env = [(s in bar,
    dB)] gain points (linear in dB between) applied to the crash voice only;
    stems = True returns {"crash": take, "rest": take} at 48 kHz instead of writing."""
    opts = opts or {}
    with open(js) as f:
        tr = json.load(f)
    kit = kit or Kit(oh)
    Tref = np.array(tr["reference_signatures_KBCY"]).T
    rng = np.random.default_rng(seed)
    order = {k: [fs[i] for i in rng.permutation(len(fs))] for k, fs in kit.files.items()}
    pos = {k: 0 for k in order}
    nudges = nudges or {}
    bar_s = BAR / SR
    take = np.zeros((int((4 * bar_s + 4.5) * KIT_SR), 2))
    crash_take = np.zeros_like(take)
    for idx, h in enumerate(tr["hits"]):
        inst, layer = h["instrument"], h["layer"]
        tom = opts.get("tom")
        as_tom = tom and h["bar"] == tom["bar"] and abs(h["step"] - tom["step"]) < 0.05
        play, play_layer = (tom.get("inst", "tom"), tom["layer"]) if as_tom else (inst, layer)
        q = order[(play, play_layer)]
        path = q[pos[(play, play_layer)] % len(q)]       # seeded round-robin
        pos[(play, play_layer)] += 1
        x, onset, spec, attack, floor = kit.sample(path)
        # gain: match the hit's added energy in the bands its instrument owns
        if inst == "crash":
            # match the wash that holds up after the stroke (over the bars' background)
            g = np.sqrt(10 ** (h["wash_floor_db"] / 10) / floor)
        else:
            k = INSTS.index(inst)
            own = {"kick": [0, 1], "snare": [1, 2], "ride": [3]}[inst]
            target = 10 ** (h["activation_db"] / 10) * Tref[own, k].sum()
            g = np.sqrt(target / (FEAT @ spec)[own].sum())   # the tom takes the kick's level
            if as_tom:
                g *= 10 ** (tom.get("gain_db", 0.0) / 20)
        # a hit on the bar line starts its attack exactly there (the cuts start on it);
        # anything else lands with its detected onset on the measured time
        lead = attack if h["time"] < 0.002 else onset
        when = (h["bar"] - 1) * bar_s + h["time"] + nudges.get(idx, 0.0) - lead
        start = int(round(when * KIT_SR))
        y = x * g
        if play == "snare" and opts.get("snare_eq"):
            y = ss.sosfilt(eq_sos(opts["snare_eq"]), y, axis=0)
        if inst == "crash" and opts.get("crash_env"):
            pts = np.array(opts["crash_env"])
            tt = (start + np.arange(len(y))) / KIT_SR - (h["bar"] - 1) * bar_s
            y = y * (10 ** (np.interp(tt, pts[:, 0], pts[:, 1]) / 20))[:, None]
        if inst == "crash":
            for c in tr.get("chokes", []):
                if c["bar"] == h["bar"] and abs(c["crash_time"] - h["time"]) < 1e-6:
                    k0 = int(round(((h["bar"] - 1) * bar_s + c["choke_time"]) * KIT_SR)) - start
                    fade = int(0.008 * KIT_SR)
                    env = np.ones(len(y))
                    if 0 < k0 < len(y):
                        n = len(env[k0:k0 + fade])
                        env[k0:k0 + n] = np.cos(np.linspace(0, np.pi / 2, n)) ** 2
                        env[k0 + n:] = 0
                    y = y * env[:, None]
        if start < 0:
            y, start = y[-start:], 0
        n = min(len(y), len(take) - start)
        (crash_take if inst == "crash" else take)[start:start + n] += y[:n]
    if opts.get("stems"):
        return {"crash": crash_take, "rest": take}
    take += crash_take
    if opts.get("eq"):
        take = ss.sosfilt(eq_sos(opts["eq"]), take, axis=0)   # before normalising
    t44 = to44(take)[:4 * BAR]
    t44 *= 10 ** (-1 / 20) / np.abs(t44).max()      # -1 dBFS over the whole take
    os.makedirs(out_dir, exist_ok=True)
    paths = []
    for b in range(4):
        p = os.path.join(out_dir, OUT_NAME.format(b + 1))
        write24(p, t44[b * BAR:(b + 1) * BAR])
        paths.append(p)
    write24(os.path.join(out_dir, "preview_4bars_x2.wav"), np.concatenate([t44, t44]))
    return paths


# ---------------------------------------------------------------- verify

def verify(js, out_dir, oh, tol=0.002, quiet=False, kit=None):
    """Same detector on the output (calibrated with the kit's own signatures)."""
    with open(js) as f:
        tr = json.load(f)
    kit = kit or Kit(oh)
    paths = [os.path.join(out_dir, OUT_NAME.format(b + 1)) for b in range(4)]
    got, _, _, _ = analyse(paths, templates=kit.templates(), bw=KIT_W)
    report, errs = [], {}
    for bi in range(4):
        ref = [(i, h) for i, h in enumerate(tr["hits"]) if h["bar"] == bi + 1]
        out = list(got[bi])
        pairs = []
        for i, h in ref:
            for j, o in enumerate(out):
                d = o["t"] - h["time"]
                if o["inst"] == h["instrument"] and abs(d) < 0.012:
                    pairs.append((abs(d), i, j, d))
        pairs.sort()
        ui, uj, best = set(), set(), {}
        for ad, i, j, d in pairs:                       # greedy one-to-one
            if i in ui or j in uj:
                continue
            ui.add(i)
            uj.add(j)
            best[i] = d
        errs.update(best)
        e = [abs(best[i]) for i, _ in ref if i in best and abs(best[i]) <= tol]
        missing = [(h, best.get(i)) for i, h in ref if not (i in best and abs(best[i]) <= tol)]
        extra = [o for j, o in enumerate(out) if j not in uj]
        report.append({"bar": bi + 1, "ref": len(ref), "matched": len(e),
                       "mean_ms": float(np.mean(e) * 1000) if e else None,
                       "max_ms": float(np.max(e) * 1000) if e else None,
                       "missing": [{"step": h["step"], "inst": h["instrument"], "vel": h["velocity"],
                                    "err_ms": None if d is None else round(d * 1000, 2)}
                                   for h, d in missing],
                       "extra": [{"step": round(o["t"] / STEP, 2), "inst": o["inst"],
                                  "rel": o["rel"]} for o in extra]})
        if not quiet:
            r = report[-1]
            print(f"bar {bi + 1}: {r['matched']}/{r['ref']} matched within {tol * 1000:.0f} ms"
                  + (f", mean {r['mean_ms']:.2f} ms, max {r['max_ms']:.2f} ms" if e else ""))
            for h, d in missing:
                print(f"   MISS  step {h['step']:6.2f} {h['instrument']:5s} {h['velocity']:6s} "
                      + (f"nearest same-class hit {d * 1000:+.2f} ms" if d is not None else "no output hit"))
            for o in extra:
                print(f"   EXTRA step {o['t'] / STEP:6.2f} {o['inst']:5s} {o['rel']:6.1f} dB")
    return report, errs


def run_all(ref_dir, oh, work, passes=6):
    js = os.path.join(work, "transcription.json")
    transcribe(ref_dir, oh, js)
    kit = Kit(oh)
    out = os.path.join(work, "out2")
    nudges, best = {}, (-1, {})
    for it in range(passes):
        render(js, oh, out, nudges, kit=kit)
        print(f"\n== verify pass {it + 1}")
        rep, errs = verify(js, out, oh, kit=kit)
        score = sum(r["matched"] for r in rep)
        if score > best[0]:
            best = (score, dict(nudges))
        if all(r["matched"] == r["ref"] for r in rep):
            break
        # closed loop: move each hit so the detector finds it where the reference has it
        for i, d in errs.items():
            if abs(d) > 0.0002:
                nudges[i] = float(np.clip(nudges.get(i, 0.0) - d, -0.008, 0.008))
    if best[1] != nudges or score < best[0]:
        nudges = best[1]
        render(js, oh, out, nudges, kit=kit)
        print("\n== final (best pass re-rendered)")
        rep, _ = verify(js, out, oh, kit=kit)
    with open(os.path.join(work, "verify.json"), "w") as f:
        json.dump({"report": rep, "nudges_ms": {str(k): round(v * 1000, 3)
                                                 for k, v in sorted(nudges.items())}}, f, indent=1)


# ---------------------------------------------------------------- out3: tone, tom, crash cut

THIRD = 1000 * 2 ** (np.arange(-15, 14) / 3)        # third-octave centres, 31 Hz-20 kHz
EQ_SHAPE = [("lowshelf", 110, 0.7), ("peak", 300, 0.9), ("peak", 4000, 0.7),
            ("highshelf", 14000, 0.7)]               # broad filters; gains are fitted
SNARE_SHAPE = [("peak", 4000, 0.7)]
# Bar 3's crash, shaped like the reference: its >7 kHz wash falls ~5 dB in the mix
# from step 5.76 to 5.82 (the crash is ~80% of that band there, so ~-9 dB on the crash
# itself; -7 dB measured closest), sits there to step 6.06, and comes back by 6.14 to within ~2 dB.
CRASH_ENV_STEPS = [(0.0, 0.0), (5.76, 0.0), (5.82, -7.0), (6.06, -7.0), (6.14, -2.0),
                   (16.0, -2.0)]
TOM = {"bar": 4, "step": 9.14, "layer": "F"}  # hiTom: 144 Hz, the hit's measured pitch


def third_octave(x):
    f, P = ss.welch(x, SR, nperseg=8192)
    return np.array([P[(f >= c / 2 ** (1 / 6)) & (f < c * 2 ** (1 / 6))].sum() for c in THIRD])


def tone_diff(ref, out):
    """Reference minus clone, dB per third octave, each normalised to its total power."""
    r, o = third_octave(ref), third_octave(out)
    return db(r / r.sum()) - db(o / o.sum())


def eq_response(filters, freqs=THIRD):
    _, h = ss.sosfreqz(eq_sos(filters), worN=freqs, fs=KIT_SR)
    return 20 * np.log10(np.abs(h))


def fit_gains(diff, shape, lo=40, hi=16000, limit=6.0):
    """Gains for the given broad filters that best match diff (dB, up to an offset)."""
    sel = (THIRD >= lo) & (THIRD <= hi)
    def res(g):
        r = eq_response([(k, f, gg, q) for (k, f, q), gg in zip(shape, g)])[sel] - diff[sel]
        return r - r.mean()
    g = so.least_squares(res, np.zeros(len(shape)), bounds=(-limit, limit)).x
    return [(k, f, round(float(gg), 1), q) for (k, f, q), gg in zip(shape, g)]


def bars_mono(paths):
    return np.concatenate([load(p)[1].mean(1) for p in paths])


def make_out3(ref_dir, oh, work):
    js = os.path.join(work, "transcription.json")
    with open(os.path.join(work, "verify.json")) as f:
        nudges = {int(k): v / 1000 for k, v in json.load(f)["nudges_ms"].items()}
    kit = Kit(oh)
    refs = [os.path.join(ref_dir, n + ".wav") for n in REF_NAMES]
    ref = bars_mono(refs)
    crash_env = [(st * STEP, g) for st, g in CRASH_ENV_STEPS]
    base = {"tom": TOM, "crash_env": crash_env}
    out3 = os.path.join(work, "out3")
    scratch = os.path.join(work, "out3_try")

    def outs(d):
        return [os.path.join(d, OUT_NAME.format(b + 1)) for b in range(4)]

    def rms(d, lo=60, hi=12000):
        sel = (THIRD >= lo) & (THIRD <= hi)
        return float(np.sqrt(np.mean((d[sel] - d[sel].mean()) ** 2)))

    render(js, oh, scratch, nudges, kit=kit, opts=base)
    d0 = tone_diff(ref, bars_mono(outs(scratch)))
    # A: broad EQ on the whole mix
    eqA = fit_gains(d0, EQ_SHAPE)
    render(js, oh, scratch, nudges, kit=kit, opts=dict(base, eq=eqA))
    dA = tone_diff(ref, bars_mono(outs(scratch)))
    # B: crack from the snare itself (per-hit peak), the rest from the broad mix EQ
    render(js, oh, scratch, nudges, kit=kit, opts=dict(base, eq=[f for f in eqA if f[1] != 4000],
                                                          snare_eq=[("peak", 4000, 6.0, 0.7)]))
    dB0 = tone_diff(ref, bars_mono(outs(scratch)))
    eqB = [f for f in eqA if f[1] != 4000] + fit_gains(dB0, SNARE_SHAPE[:0] + [("peak", 4000, 0.7)])
    optsB = dict(base, eq=eqB, snare_eq=[("peak", 4000, 6.0, 0.7)])
    render(js, oh, scratch, nudges, kit=kit, opts=optsB)
    dB = tone_diff(ref, bars_mono(outs(scratch)))
    print("third-octave ref - clone (dB):")
    print("   Hz   out2-ish  A:mixEQ  B:snareEQ")
    for c, a, b_, cc in zip(THIRD, d0, dA, dB):
        print(f"{c:6.0f} {a:8.1f} {b_:8.1f} {cc:8.1f}")
    print(f"rms deviation 60 Hz-12 kHz: before {rms(d0):.1f}  A {rms(dA):.1f}  B {rms(dB):.1f}")
    print("A eq:", eqA)
    print("B eq:", eqB, "+ snare peak 4 kHz +6 dB")
    choice, opts = ("A", dict(base, eq=eqA)) if rms(dA) <= rms(dB) else ("B", optsB)
    render(js, oh, out3, nudges, kit=kit, opts=opts)
    for p in outs(scratch) + [os.path.join(scratch, "preview_4bars_x2.wav")]:
        os.remove(p)
    os.rmdir(scratch)
    print("chose", choice)
    rep, _ = verify(js, out3, oh, kit=Kit(oh, eq=opts.get("eq"), snare_eq=opts.get("snare_eq")))
    print("per-bar tone (rms dev of ref-clone third octaves, 60 Hz-12 kHz; out2 -> out3):")
    for b in range(4):
        r = load(refs[b])[1].mean(1)
        o2 = load(outs(os.path.join(work, "out2"))[b])[1].mean(1)
        o3 = load(outs(out3)[b])[1].mean(1)
        print(f"  bar {b + 1}: {rms(tone_diff(r, o2)):.1f} -> {rms(tone_diff(r, o3)):.1f} dB")
    # local A/B only: contains reference audio, lives only under target/, never shipped
    ab = np.concatenate([np.concatenate([load(refs[b])[1], load(outs(out3)[b])[1]]) for b in range(4)])
    write24(os.path.join(work, "LOCAL_ONLY_contains_reference_audio",
                         "compare_ref_vs_clone.wav") if os.makedirs(
        os.path.join(work, "LOCAL_ONLY_contains_reference_audio"), exist_ok=True) is None else "", ab)
    with open(os.path.join(work, "out3_settings.json"), "w") as f:
        json.dump({"choice": choice, "opts": {k: v for k, v in opts.items()}, "report": rep}, f, indent=1)


# ---------------------------------------------------------------- native originals

NATIVE = ["KAB1_135_AmenBreak_Cut_01", "KAB1_137_AmenBreak_Cut_02",
          "KAB1_137_AmenBreak_Cut_03", "KAB1_139_AmenBreak_Cut_04"]
NATIVE_FULL = "KAB1_0_AmenBreak_Original_FullSample"
# native-only hits are added only when their onset is unmistakable (rise, dB)
NATIVE_ADD_RISE = {"kick": 10.0, "snare": 10.0, "ride": 8.0}
NATIVE_ADD_LEVEL = {"kick": -10.0, "snare": -13.0, "ride": -10.0}
NATIVE_TIME_RISE = 8.0     # a matched hit takes the native time only from a clear onset
NATIVE_TIME_MIN = 0.005    # ... and only when the stretch moved it by more than 5 ms


def locate(full, cut):
    """Best normalised-correlation position of cut in full: (index, r)."""
    num = ss.fftconvolve(full, cut[::-1], "valid")
    e = np.concatenate([[0.0], np.cumsum(full ** 2)])
    r = num / (np.sqrt((e[len(cut):] - e[:-len(cut)]) * np.dot(cut, cut)) + 1e-12)
    i = int(np.argmax(r))
    return i, float(r[i])


def make_native(orig_dir, oh, work):
    """Transcribe KAN's un-stretched cuts (analysis only), map them onto the 174 bar,
    and merge with the stretched-cut transcription where the native one is clearer."""
    full = load(os.path.join(orig_dir, NATIVE_FULL + ".wav"))[1].mean(1)
    cuts = [load(os.path.join(orig_dir, n + ".wav"))[1].mean(1) for n in NATIVE]
    print("placement in the full sample (start s, correlation):")
    pos = []
    for n, c in zip(NATIVE, cuts):
        i, r = locate(full, c)
        pos.append(i)
        print(f"  {n}: {i / SR:.4f} s r={r:.3f}  file {len(c)} smp = {240 * SR / len(c):.2f} BPM")
    # cut 3 is not in the full sample: it is cut 2 (at about half level) with a crash on top
    xc = ss.fftconvolve(cuts[2], cuts[1][::-1], "full")
    lag3 = int(np.argmax(xc)) - (len(cuts[1]) - 1)
    c2s = np.concatenate([np.zeros(lag3), cuts[1]])[:len(cuts[2])]
    g3 = float(np.dot(cuts[2], c2s) / np.dot(c2s, c2s))
    print(f"  cut 3 = {g3:.2f} x cut 2 delayed {lag3} smp + a crash")
    nxt = {0: pos[1], 1: pos[1] + len(cuts[1]), 3: None}
    for b, n in nxt.items():
        if n:
            print(f"  cut {b + 1} downbeat-to-downbeat {n - pos[b]} smp = {240 * SR / (n - pos[b]):.2f} BPM")
    before = [full[max(0, p - 2205):p] for p in pos]
    before[2] = full[pos[1] - lag3 - 2205:pos[1] - lag3] * g3
    kit = Kit(oh)
    res, T, used, bars = analyse([os.path.join(orig_dir, n + ".wav") for n in NATIVE], kit,
                                 ride_cy_max=None, before=before, merge_keep="rise")
    with open(os.path.join(work, "transcription.json")) as f:
        tr = json.load(f)
    st = tr["hits"]
    # map: the stretch maps each whole file onto one 174 bar
    nat = []
    for b in range(4):
        sc = BAR / len(bars[b])
        for h in res[b]:
            nat.append(dict(h, bar=b + 1, t174=h["t"] * sc))
    # match native to stretched: same instrument within 12 ms
    pairs, used_s, used_n = [], set(), set()
    cand = sorted((abs(n["t174"] - s["time"]), i, j) for i, n in enumerate(nat)
                  for j, s in enumerate(st) if n["bar"] == s["bar"]
                  and n["inst"] == s["instrument"] and abs(n["t174"] - s["time"]) < 0.012)
    for _, i, j in cand:
        if i not in used_n and j not in used_s:
            used_n.add(i)
            used_s.add(j)
            pairs.append((i, j))
    off = {inst: float(np.median([st[j]["activation_db"] - nat[i]["act"] for i, j in pairs
                                  if nat[i]["inst"] == inst] or [0.0])) for inst in INSTS}
    out, log = [], []
    for i, j in pairs:
        s, n = st[j], nat[i]
        if n["rise"] < NATIVE_TIME_RISE or abs(n["t174"] - s["time"]) <= NATIVE_TIME_MIN:
            # weak native onset, or a shift too small to matter: keep the stretched time
            # (keeps the clone comparable with the 174 cuts)
            out.append(dict(s, src="both, stretched time"))
            continue
        out.append(dict(s, time=round(n["t174"], 6), step=round(n["t174"] / STEP, 3), src="both"))
        if abs(n["t174"] - s["time"]) > 0.002:
            log.append(f"bar {s['bar']} {s['instrument']:5s} {s['step']:6.2f} -> {n['t174'] / STEP:6.2f} "
                       f"(timing {1000 * (n['t174'] - s['time']):+.1f} ms)")
    for i, n in enumerate(nat):
        if i in used_n or n["inst"] not in NATIVE_ADD_RISE or n["t"] < 0.002:
            continue                  # (a cut-start hit's class can't be checked: not added)
        # the ride under a backbeat snare has a soft onset (masked by the snare's ring)
        # but sits there in every native bar: take it if it's loud enough
        under_snare = n["inst"] == "ride" and n["rel"] >= -8 and any(
            m["inst"] == "snare" and m["bar"] == n["bar"] and 0.012 < n["t174"] - m["t174"] < 0.040
            and m["rel"] >= -5 for m in nat)
        if not under_snare and (n["rise"] < NATIVE_ADD_RISE[n["inst"]]
                                or n["rel"] < NATIVE_ADD_LEVEL[n["inst"]]):
            continue
        if n["inst"] == "snare" and any(m["inst"] == "ride" and m["bar"] == n["bar"]
                                        and abs(m["t174"] - n["t174"]) < 0.015 for m in nat):
            continue                  # native ride crack is snare-like; don't trust these
        act = n["act"] + off[n["inst"]]
        vel, layer = next((v, l) for th, v, l in VEL[n["inst"]] if n["rel"] >= th)
        out.append({"bar": n["bar"], "time": round(n["t174"], 6), "step": round(n["t174"] / STEP, 3),
                    "instrument": n["inst"], "velocity": vel, "layer": layer,
                    "level_db": round(n["rel"], 2), "activation_db": round(act, 2), "src": "native"})
        log.append(f"bar {n['bar']} {n['inst']:5s} added at {n['t174'] / STEP:6.2f} ({vel}, {n['rel']:.0f} dB)")
    for j, s in enumerate(st):
        if j in used_s:
            continue
        if s["time"] < 0.002:        # the bar's own downbeat stays: a slice starts on it
            out.append(dict(s, src="stretched"))
            continue
        near = [n for n in nat if n["bar"] == s["bar"] and n["inst"] == s["instrument"]
                and abs(n["t174"] - s["time"]) < 0.040 and n["rise"] >= NATIVE_ADD_RISE[n["inst"]]]
        if near:
            log.append(f"bar {s['bar']} {s['instrument']:5s} {s['step']:6.2f} dropped "
                       f"(stretch smear of the native hit at {near[0]['t174'] / STEP:.2f})")
            continue
        out.append(dict(s, src="stretched"))
    # a hit moved or added can now sit on top of another of its kind: keep the first
    out.sort(key=lambda h: (h["bar"], h["time"]))
    final = []
    for h in out:
        if any(o["bar"] == h["bar"] and o["instrument"] == h["instrument"]
               and abs(o["time"] - h["time"]) < MERGE[h["instrument"]] for o in final):
            log.append(f"bar {h['bar']} {h['instrument']:5s} {h['step']:6.2f} merged (duplicate)")
            continue
        final.append(h)
    js = dict(tr, hits=final, chokes=[], source="native-derived (KAN originals, analysis only) "
              "merged with the stretched-cut transcription")
    with open(os.path.join(work, "transcription_native.json"), "w") as f:
        json.dump(js, f, indent=1)
    print("native exemplars:", used, " level offset native->stretched:", off)
    print("changes:")
    for line in log:
        print("  " + line)
    print_table(final)
    return js


# ---------------------------------------------------------------- out4: lean low end, tom/kick

# The break shouldn't carry the bass (Junglizer's 808 layer does): a fixed high-pass at
# 85 Hz (two 12 dB/oct stages: Salamander's kick sits at 51 Hz, one stage left it louder
# than the reference there) plus a low shelf at 60 Hz take the deep sub out, leaner than
# the reference under 60 Hz on purpose; a fixed gentle top shelf keeps 12 kHz+ from
# brightening; the 100 Hz thump, 300 Hz body and 4.5 kHz crack are fitted 70 Hz-12 kHz,
# uncapped.
OUT4_FIXED = [("highpass", 85, 0.0, 0.707), ("highpass", 85, 0.0, 0.707),
              ("lowshelf", 60, -20.0, 0.7), ("highshelf", 14000, -3.0, 0.7)]
OUT4_SHAPE = [("peak", 100, 1.0), ("peak", 300, 0.9), ("peak", 4500, 0.7)]
OUT4_SNARE_EQ = [("peak", 4000, 6.0, 0.7)]   # the crack from the snare itself (as out3)
LOTOM = {"bar": 4, "step": 9.14, "inst": "lotom", "layer": "MP"}
# Bar 3's crash as measured on the native cut 3 (= cut 2 + a crash; the difference is
# the crash alone): flat to step 5.80, down ~10 dB (>7 kHz) / ~15 dB (2-6 kHz) by 5.89,
# still down at 6.22 where the ride masks it. A real cut, not a stretch artifact; the
# stretched cut's "recovery" was the ride. After 6.22 it is assumed gone.
CRASH_ENV_NATIVE = [(0.0, 0.0), (5.80, 0.0), (5.89, -13.0), (6.22, -13.0), (7.0, -30.0),
                    (16.0, -30.0)]


def hit_level(m_bars, bar, step):
    """Added <1 kHz energy (dB) of the event nearest the step, re bar's downbeat."""
    evs = bar_events(m_bars)
    near = min((e for e in evs if e["bar"] == bar - 1), key=lambda e: abs(e["t"] / STEP - step))
    down = min((e for e in evs if e["bar"] == bar - 1), key=lambda e: e["t"])
    return float(db(near["f"][:2].sum()) - db(down["f"][:2].sum()))


def make_out4(ref_dir, oh, work):
    js_str = os.path.join(work, "transcription.json")
    js = os.path.join(work, "transcription_native.json")
    with open(os.path.join(work, "verify.json")) as f:
        old = {int(k): v / 1000 for k, v in json.load(f)["nudges_ms"].items()}
    with open(js_str) as f:
        st = json.load(f)["hits"]
    with open(js) as f:
        nh = json.load(f)["hits"]
    # carry the closed-loop nudges over to hits that kept their stretched time
    key = {(h["bar"], h["instrument"], h["time"]): old.get(i, 0.0) for i, h in enumerate(st)}
    nudges = {i: key.get((h["bar"], h["instrument"], h["time"]), 0.0) for i, h in enumerate(nh)}
    kit = Kit(oh)
    refs = [os.path.join(ref_dir, n + ".wav") for n in REF_NAMES]
    ref_bars = [load(p)[1].mean(1) for p in refs]
    ref = np.concatenate(ref_bars)
    base = {"crash_env": [(x * STEP, g) for x, g in CRASH_ENV_NATIVE], "snare_eq": OUT4_SNARE_EQ}
    root = os.path.join(work, "out4")
    tmp = os.path.join(root, "try")

    def outs(d):
        return [os.path.join(d, OUT_NAME.format(b + 1)) for b in range(4)]

    def rms(d, lo=60, hi=12000):
        sel = (THIRD >= lo) & (THIRD <= hi)
        return float(np.sqrt(np.mean((d[sel] - d[sel].mean()) ** 2)))

    def sub(d):                      # bands under 60 Hz, re the 60 Hz-12 kHz mean
        sel = (THIRD >= 60) & (THIRD <= 12000)
        return " ".join(f"{c:.0f}:{v - d[sel].mean():+.1f}" for c, v in zip(THIRD, d) if c < 60)

    # fit the mix EQ on top of the fixed high-pass and the snare EQ
    render(js, oh, tmp, nudges, kit=kit, opts=dict(base, eq=OUT4_FIXED, tom=LOTOM))
    d_hp = tone_diff(ref, bars_mono(outs(tmp)))
    eq = OUT4_FIXED + fit_gains(d_hp, OUT4_SHAPE, lo=70, hi=12000, limit=18.0)
    # loTom level: about the reference hit's, a touch under (it should sit back)
    ref_lvl = hit_level(ref_bars, 4, 9.14)
    tom = dict(LOTOM, gain_db=0.0)
    for _ in range(3):
        render(js, oh, tmp, nudges, kit=kit, opts=dict(base, eq=eq, tom=tom))
        got = hit_level([load(p)[1].mean(1) for p in outs(tmp)], 4, 9.14)
        tom["gain_db"] = round(tom["gain_db"] + (ref_lvl - 1.0) - got, 2)
    variants = {"a": dict(base, eq=eq, tom=tom), "b": dict(base, eq=eq)}
    vkit = Kit(oh, eq=eq, snare_eq=OUT4_SNARE_EQ)
    # closed loop on the timing (as in run_all), against the transcription we render
    best = (-1, dict(nudges))
    for it in range(4):
        render(js, oh, tmp, nudges, kit=kit, opts=variants["b"])
        rep, errs = verify(js, tmp, oh, kit=vkit, quiet=True)
        score = sum(r["matched"] for r in rep)
        print(f"timing pass {it + 1}: {score} matched")
        if score > best[0]:
            best = (score, dict(nudges))
        for i, dd in errs.items():
            if abs(dd) > 0.0002:
                nudges[i] = float(np.clip(nudges.get(i, 0.0) - dd, -0.008, 0.008))
    nudges = best[1]
    reports = {}
    for v, opts in variants.items():
        d = os.path.join(root, v)
        render(js, oh, d, nudges, kit=kit, opts=opts)
        os.replace(os.path.join(d, "preview_4bars_x2.wav"),
                   os.path.join(root, f"preview_4bars_x2_{v}.wav"))
        print(f"\n== verify out4{v} against the stretched 174 cuts")
        reports[v], _ = verify(js_str, d, oh, kit=vkit)
        print(f"== verify out4{v} against the native-derived transcription")
        reports[v + "_native"], _ = verify(js, d, oh, kit=vkit)
    final_lvl = hit_level([load(p)[1].mean(1) for p in outs(os.path.join(root, "a"))], 4, 9.14)
    for p in outs(tmp) + [os.path.join(tmp, "preview_4bars_x2.wav")]:
        os.remove(p)
    os.rmdir(tmp)
    print("\nmix EQ:", eq, "+ snare", OUT4_SNARE_EQ)
    print(f"loTom {LOTOM['layer']} gain {tom['gain_db']:+.1f} dB: hit {final_lvl:.1f} dB re downbeat "
          f"(reference hit {ref_lvl:.1f} dB)")
    o3 = bars_mono(outs(os.path.join(work, "out3")))
    o4 = bars_mono(outs(os.path.join(root, "a")))
    d3, d4 = tone_diff(ref, o3), tone_diff(ref, o4)
    print("third-octave ref - clone (dB), out3 -> out4a:")
    print("  " + " ".join(f"{c:.0f}:{a:+.1f}>{b:+.1f}" for c, a, b in zip(THIRD, d3, d4)))
    print(f"all bars 60 Hz-12 kHz deviation {rms(d3):.1f} -> {rms(d4):.1f}; "
          f"<60 Hz out3 [{sub(d3)}] out4a [{sub(d4)}]")
    for b in range(4):
        o3b = load(outs(os.path.join(work, "out3"))[b])[1].mean(1)
        o4b = load(outs(os.path.join(root, "a"))[b])[1].mean(1)
        print(f"  bar {b + 1}: {rms(tone_diff(ref_bars[b], o3b)):.1f} -> "
              f"{rms(tone_diff(ref_bars[b], o4b)):.1f} dB")
    # local A/B only: contains reference audio, lives only under target/, never shipped
    ab_dir = os.path.join(work, "LOCAL_ONLY_contains_reference_audio")
    os.makedirs(ab_dir, exist_ok=True)
    a_bars = outs(os.path.join(root, "a"))
    write24(os.path.join(ab_dir, "compare_ref_vs_clone.wav"),
            np.concatenate([np.concatenate([load(refs[b])[1], load(a_bars[b])[1]]) for b in range(4)]))
    with open(os.path.join(root, "out4_settings.json"), "w") as f:
        json.dump({"eq": eq, "snare_eq": OUT4_SNARE_EQ, "tom_a": tom, "reports": reports}, f, indent=1)


# ---------------------------------------------------------------- reproducible build

HERE = os.path.dirname(os.path.abspath(__file__))
SEED = 174


def _settings_opts(st):
    """render() opts from render_settings.json (the out4 'b' choice: no tom)."""
    return {"eq": [tuple(f) for f in st["eq"]], "snare_eq": [tuple(f) for f in st["snare_eq"]],
            "crash_env": [tuple(p) for p in st["crash_env"]]}


def bake(work, oh, out):
    """Replay out4's closed-loop timing for choice b from saved JSON (no reference audio:
    verify() compares the render with the transcription) and bake the final nudges into
    each hit's time. Writes transcription.json and render_settings.json to out."""
    with open(os.path.join(work, "out4", "out4_settings.json")) as f:
        o4 = json.load(f)
    with open(os.path.join(work, "verify.json")) as f:
        old = {int(k): v / 1000 for k, v in json.load(f)["nudges_ms"].items()}
    js_str = os.path.join(work, "transcription.json")
    js = os.path.join(work, "transcription_native.json")
    with open(js_str) as f:
        st = json.load(f)["hits"]
    with open(js) as f:
        tr = json.load(f)
    nh = tr["hits"]
    key = {(h["bar"], h["instrument"], h["time"]): old.get(i, 0.0) for i, h in enumerate(st)}
    nudges = {i: key.get((h["bar"], h["instrument"], h["time"]), 0.0) for i, h in enumerate(nh)}
    settings = {"choice": "b", "seed": SEED, "eq": o4["eq"], "snare_eq": o4["snare_eq"],
                "crash_env": [(x * STEP, g) for x, g in CRASH_ENV_NATIVE]}
    opts = _settings_opts(settings)
    kit = Kit(oh)
    vkit = Kit(oh, eq=opts["eq"], snare_eq=opts["snare_eq"])
    tmp = os.path.join(out, "try")
    best = (-1, dict(nudges))
    for it in range(4):
        render(js, oh, tmp, nudges, kit=kit, opts=opts)
        rep, errs = verify(js, tmp, oh, kit=vkit, quiet=True)
        score = sum(r["matched"] for r in rep)
        print(f"timing pass {it + 1}: {score} matched")
        if score > best[0]:
            best = (score, dict(nudges))
        for i, dd in errs.items():
            if abs(dd) > 0.0002:
                nudges[i] = float(np.clip(nudges.get(i, 0.0) - dd, -0.008, 0.008))
    nudges = best[1]
    for p in glob.glob(os.path.join(tmp, "*.wav")):
        os.remove(p)
    os.rmdir(tmp)
    hits = []
    for i, h in enumerate(nh):
        n = nudges.get(i, 0.0)
        if h["time"] < 0.002:
            assert n == 0.0, (i, n)       # bar-line hits are placed by their attack
        hits.append(dict(h, time=round(h["time"] + n, 9)))
    baked = dict(tr, hits=hits, nudges_baked=True,
                 source="transcription of an original amen (hit times, instrument, level only; "
                        "no audio), with the closed-loop timing nudges baked into each time")
    with open(os.path.join(out, "transcription.json"), "w") as f:
        json.dump(baked, f, indent=1)
    with open(os.path.join(out, "render_settings.json"), "w") as f:
        json.dump(settings, f, indent=1)
    print("nonzero nudges:", sum(1 for v in nudges.values() if v), "best score", best[0])


def render_final(oh, out_dir):
    """The shipped amen: reads only transcription.json and render_settings.json here,
    plus the kit folder. Fixed seed, the out4 'b' options."""
    with open(os.path.join(HERE, "render_settings.json")) as f:
        st = json.load(f)
    assert st["choice"] == "b"
    return render(os.path.join(HERE, "transcription.json"), oh, out_dir,
                  seed=st.get("seed", SEED), opts=_settings_opts(st))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "transcribe":
        transcribe(*sys.argv[2:5])
    elif cmd == "render":
        render(*sys.argv[2:5])
    elif cmd == "verify":
        verify(sys.argv[2], sys.argv[3], sys.argv[4])
    elif cmd == "all":
        run_all(*sys.argv[2:5])
    elif cmd == "out3":
        make_out3(*sys.argv[2:5])
    elif cmd == "out4":
        make_out4(*sys.argv[2:5])
    elif cmd == "native":
        make_native(*sys.argv[2:5])
    elif cmd == "bake":
        bake(*sys.argv[2:5])
    elif cmd == "render-final":
        render_final(*sys.argv[2:4])
