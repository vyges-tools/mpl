# SPDX-License-Identifier: Apache-2.0
"""The case's minimum-channel commands, as `cluster-dump` flags — shared by every scorer.

⛔ **These are COMMAND state, not database state.** Preparing the `.odb` keeps `set_halo` (it
writes the instance) and nothing of what is translated here, so a scorer that skips one runs the
engine on a different input than the reference used.

Every spelling ends in the reference's one setter, `set_min_channel width height` (microns), and
the LAST call wins:

| spelling                          | channel                                  |
| ---                               | ---                                      |
| `set_macro_base_halo l b [r t]`   | `(l + r, b + t)` — a halo is half a channel; two values mirror |
| `-halo_width w` / `-halo_height h`| `(2w, 2h)`; one given sets the other to it |
| `-min_channel_size {w [h]}`       | `(w, h)`; one value is square — applied after the halo keys |

`-pin_aware_channels` is a flag of its own. Values stay in MICRONS: the binary converts.
"""
import re

BASE_HALO = re.compile(r"set_macro_base_halo\s+([0-9.\s]+)")
HALO_WIDTH = re.compile(r"-halo_width\s+([0-9.]+)")
HALO_HEIGHT = re.compile(r"-halo_height\s+([0-9.]+)")
MIN_CHANNEL = re.compile(r"-min_channel_size\s+(?:\{([^}]*)\}|([0-9.]+))")

# Anything in a .tcl from this list that the helper did not translate is being silently ignored.
KNOWN = ("set_macro_base_halo", "-halo_width", "-halo_height", "-min_channel_size",
         "-pin_aware_channels")


def channel_flags(text):
    # ⛔ Comments first. `halos4.tcl` opens with `# Test that the default (no -pin_aware_channels)
    # ...`, and a substring test on the raw text turned pin-awareness ON for the one case written
    # to prove it is off. Its halos came out the same either way (no channel, so the pin-aware
    # halo IS the spacing); only the orientation branch showed it.
    text = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
    channel = None
    for vals in BASE_HALO.findall(text):
        v = [float(x) for x in vals.split()]
        if len(v) == 2:
            v = [v[0], v[1], v[0], v[1]]
        channel = (v[0] + v[2], v[1] + v[3])
    w, h = HALO_WIDTH.search(text), HALO_HEIGHT.search(text)
    if w or h:
        hw = float((w or h).group(1))
        hh = float((h or w).group(1))
        channel = (2 * hw, 2 * hh)
    m = MIN_CHANNEL.search(text)
    if m:
        v = [float(x) for x in (m.group(1) or m.group(2)).split()]
        channel = (v[0], v[-1])
    out = []
    if channel is not None:
        out += ["--min-channel", f"{channel[0]:g},{channel[1]:g}"]
    if "-pin_aware_channels" in text:
        out += ["--pin-aware-channels"]
    return out
