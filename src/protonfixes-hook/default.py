# tuxgt-proton-hook v1
"""Packaged global default, then TuxGT session env."""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tuxgt  # noqa: E402


def _packaged(stage: str) -> None:
    try:
        from protonfixes.fix import _run_fix, get_game_id

        _run_fix(get_game_id(), stage, True, False)
    except Exception:
        pass


def early() -> None:
    _packaged("early")
    tuxgt.apply()


def main() -> None:
    _packaged("main")
