# tuxgt-proton-hook v1
# wrapped-user-default
"""User localfixes/default.py, then TuxGT session env. Do not chain packaged."""

from pathlib import Path
import importlib.util
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tuxgt  # noqa: E402

_user_path = Path(__file__).with_name("_tuxgt_wrapped_default.py")
_user = None
if _user_path.is_file():
    _spec = importlib.util.spec_from_file_location("_tuxgt_wrapped_default", _user_path)
    if _spec and _spec.loader:
        _user = importlib.util.module_from_spec(_spec)
        _spec.loader.exec_module(_user)


def _call(name: str) -> None:
    if _user is None:
        return
    fn = getattr(_user, name, None)
    if callable(fn):
        fn()


def early() -> None:
    _call("early")
    tuxgt.apply()


def main() -> None:
    _call("main")
