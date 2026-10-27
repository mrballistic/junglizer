#!/usr/bin/env python3
"""SPIKE: list onset times in a WAV (stdlib only), in sixteenths at 174 BPM."""
import math, sys, wave
STEP = 60 / 174 / 4
def onsets(path, hop=64):
    w = wave.open(path); sw, ch, rate = w.getsampwidth(), w.getnchannels(), w.getframerate()
    raw = w.readframes(w.getnframes())
    mono = [sum(int.from_bytes(raw[i + c*sw:i + c*sw + sw], "little", signed=True) for c in range(ch)) / ch
            for i in range(0, len(raw), sw * ch)]
    env = [math.log(1e-9 + sum(x*x for x in mono[i:i+hop])) for i in range(0, len(mono) - hop, hop)]
    flux = [max(0, env[i] - env[i-3]) for i in range(3, len(env))]
    out, last = [], -99
    for i in range(1, len(flux) - 1):
        if flux[i] > 1.4 and flux[i] >= flux[i-1] and flux[i] >= flux[i+1] and i - last > 8:
            out.append(((i + 3) * hop / rate, flux[i])); last = i
    return out
if __name__ == "__main__":
    for p in sys.argv[1:]:
        print(p.split('/')[-1])
        for t, s in onsets(p):
            print(f"  {t*1000:7.1f} ms  step {t/STEP:5.2f}  strength {s:4.1f}")
