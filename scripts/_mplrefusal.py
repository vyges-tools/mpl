# SPDX-License-Identifier: Apache-2.0
"""When our engine stops and the reference stops the SAME way — agreement, not "other".

⛔ **Earned, never assumed.** Each rule names the reference's own log line it requires:

- `same_error`: our refusal carries `MplError { code: N }` AND the reference log has
  `[ERROR MPL-000N]` — both sides refuse the design for the same reason
  (`unfixed_cells_dont_fit_in_core`, MPL-65);
- `both_skip`: we exit vacuous (3) AND the reference log has `[INFO MPL-0017] No unfixed macros`.

A refusal with a different code, or none on the reference side, stays "other" and fails the gate.
Each scorer counts these in a bucket of its own, so a run that did nothing is never a match.
"""
import re

OURS = re.compile(r"MplError \{ code: (\d+)")


def same_error(stderr, log_text):
    """The MPL code both sides stopped on, or None."""
    m = OURS.search(stderr)
    if not m:
        return None
    code = int(m.group(1))
    return code if f"[ERROR MPL-{code:04d}]" in log_text else None


def both_skip(returncode, log_text):
    return returncode == 3 and "[INFO MPL-0017]" in log_text
