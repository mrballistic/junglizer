#!/bin/sh
# Regenerates tests/fixtures: synthetic click loops (our own audio), 4 beats at
# 160 BPM = 1.5 s, plus three deliberately bad files. Needs ffmpeg and lame.
set -eu
cd "$(dirname "$0")/../../tests/fixtures"

# One 1 kHz sine click (20 ms) every 0.375 s over 1.5 s.
SRC='aevalsrc=0.8*sin(2*PI*1000*t)*lt(mod(t\,0.375)\,0.02):d=1.5:s=44100:c=stereo'

ffmpeg -y -loglevel error -f lavfi -i "$SRC" -ac 1 -ar 48000 -c:a pcm_s16le loop_160_4beats_mono16_48k.wav
ffmpeg -y -loglevel error -f lavfi -i "$SRC" -ac 2 -ar 44100 -c:a pcm_s24be -f aiff loop_160_4beats_stereo24_44k.aif
ffmpeg -y -loglevel error -f lavfi -i "$SRC" -ac 2 -ar 48000 -c:a pcm_f32le loop_160_4beats_float32_48k.wav
ffmpeg -y -loglevel error -f lavfi -i "$SRC" -ac 2 -ar 44100 -c:a pcm_s16le -f wav - |
    lame --silent -b 128 - loop_160_4beats.mp3

head -c 100 loop_160_4beats_mono16_48k.wav > truncated.wav
printf 'not audio' > not_audio.wav
: > empty.wav
