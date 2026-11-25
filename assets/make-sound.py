"""Synthesise the sound Rec plays when you press the shortcut: one quick
knock, like a knuckle on a wooden door.

A knock is a click followed by a few wooden resonances that die out within
a tenth of a second: a low "thud" body and some short, higher overtones.

Run: python3 assets/make-sound.py src-tauri/sounds/shortcut.wav
"""
import math
import random
import struct
import sys
import wave

RATE = 44100
LENGTH = 0.22  # seconds
PEAK = 0.4

# (frequency Hz, decay time s, level): the door's body and wood overtones.
MODES = [(170, 0.045, 1.0), (285, 0.030, 0.7), (520, 0.018, 0.45), (940, 0.010, 0.3), (1700, 0.005, 0.18)]

random.seed(3)
n = int(RATE * LENGTH)
out = []
lp = 0.0
for i in range(n):
    t = i / RATE
    s = 0.0
    for f, tau, level in MODES:
        # A slight pitch drop at the hit makes it sound like a solid thud.
        bend = 1 + 0.06 * math.exp(-t / 0.008)
        s += level * math.sin(2 * math.pi * f * bend * t) * math.exp(-t / tau)
    # The knuckle's click: a 3 ms burst of softened noise.
    lp += 0.35 * (random.uniform(-1, 1) - lp)
    s += 0.8 * lp * math.exp(-t / 0.0015)
    s *= min(1.0, t / 0.0008)  # no pop at the very start
    out.append(s)

scale = PEAK / max(abs(s) for s in out)
with wave.open(sys.argv[1] if len(sys.argv) > 1 else "shortcut.wav", "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes(b"".join(struct.pack("<h", int(s * scale * 32767)) for s in out))
