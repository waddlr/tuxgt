# tuxgt-proton-hook v1
"""Shim: load tuxgt_apply from $TUXGT_DATA/share/protonfixes."""

from __future__ import annotations

import os
import sys
from pathlib import Path


def _data() -> Path:
    env = os.environ.get("TUXGT_DATA", "").strip()
    if env:
        return Path(env)
    conf = Path.home() / ".config" / "tuxgt.conf"
    if conf.is_file():
        for line in conf.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if line.startswith("TUXGT_DATA="):
                v = line.split("=", 1)[1].strip().strip('"').strip("'")
                if v:
                    return Path(v)
    return Path.home() / "tuxgt"


_d = _data() / "share" / "protonfixes"
if _d.is_dir() and str(_d) not in sys.path:
    sys.path.insert(0, str(_d))

from tuxgt_apply import apply  # noqa: E402,F401
