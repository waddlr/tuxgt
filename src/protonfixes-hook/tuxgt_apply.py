# tuxgt-proton-hook v1
"""Read games/load-correlator.ini + tux-protonfixes.conf when inject=1."""

from __future__ import annotations

import os
import sys
from collections.abc import Iterator
from pathlib import Path


SKIP = frozenset({"inject", "WRAPPERS", "LD_PRELOAD"})


def data_dir() -> Path:
    env = os.environ.get("TUXGT_DATA", "").strip()
    if env:
        return Path(env)
    conf = Path.home() / ".config" / "tuxgt.conf"
    if conf.is_file():
        for line in conf.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if line.startswith("#") or not line:
                continue
            if line.startswith("TUXGT_DATA="):
                v = line.split("=", 1)[1].strip().strip('"').strip("'")
                if v:
                    return Path(v)
    return Path.home() / "tuxgt"


def _norm(p: str) -> str:
    # R55: same canonical key as session::canonical_exe_key — separators
    # fold, case folds, then surrounding whitespace/trailing slashes trim —
    # so a Proton-reported `SkyrimSE.exe` hits a lowercased ini key.
    return p.replace("\\", "/").lower().strip().rstrip("/").strip()


def runtime_prefix(env: dict[str, str] | None = None) -> str:
    e = os.environ if env is None else env
    return _norm(e.get("STEAM_COMPAT_DATA_PATH") or e.get("WINEPREFIX") or "")


def runtime_exe(env: dict[str, str] | None = None, argv: list[str] | None = None) -> str:
    e = os.environ if env is None else env
    exe = _norm(e.get("EXE") or "")
    if exe:
        return exe
    for a in reversed(argv or []):
        if a.lower().endswith(".exe"):
            return _norm(a)
    return ""


def _parse_kv(path: Path) -> Iterator[str]:
    """Yield stripped content lines (blanks and #-comments dropped)."""
    if not path.is_file():
        return
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        yield line


def load_correlator(path: Path) -> dict[str, dict[str, str]]:
    """prefix -> (exe -> rel)."""
    out: dict[str, dict[str, str]] = {}
    section = ""
    for line in _parse_kv(path):
        if line.startswith("[") and line.endswith("]"):
            section = _norm(line[1:-1])
            out.setdefault(section, {})
            continue
        if "=" not in line or not section:
            continue
        k, v = line.split("=", 1)
        out[section][_norm(k.strip())] = v.strip()
    return out


def correlate(data: Path, env: dict[str, str] | None = None, argv: list[str] | None = None) -> Path | None:
    prefix = runtime_prefix(env)
    exe = runtime_exe(env, argv)
    if not prefix or not exe:
        return None
    section = load_correlator(data / "games" / "load-correlator.ini").get(prefix, {})
    rel = section.get(exe)
    if not rel:
        # Prefix fallback: launcher-first chains (MO2 outer, SKSE middle)
        # report an exe with no key of its own; the inner game exe
        # inherits this env, and the loader stem gate stays the decider.
        # Only a single-rel section resolves — contested prefixes miss.
        uniq = {r for r in section.values() if r}
        if len(uniq) != 1:
            return None
        rel = uniq.pop()
    conf = data / "games" / rel / "tux-protonfixes.conf"
    return conf if conf.is_file() else None


def read_session(path: Path) -> dict[str, str]:
    out: dict[str, str] = {}
    for line in _parse_kv(path):
        if "=" not in line:
            continue
        k, v = line.split("=", 1)
        k, v = k.strip(), v.strip()
        if k:
            out[k] = v
    return out


def find_session() -> dict[str, str]:
    conf = correlate(data_dir(), argv=sys.argv)
    if not conf:
        return {}
    return read_session(conf)


def _setenv(k: str, v: str) -> None:
    try:
        from protonfixes import util

        util.set_environment(k, v)
    except Exception:
        os.environ[k] = v


def _list_append(env: dict[str, str], key: str, value: str, sep: str = ":") -> None:
    if not value:
        return
    parts = [x for x in env.get(key, "").split(sep) if x]
    if value in parts:
        return
    parts.append(value)
    _setenv(key, sep.join(parts))


def _pv_rw_add(p: str) -> None:
    _list_append(os.environ, "PRESSURE_VESSEL_FILESYSTEMS_RW", p)


def _preload_append(so: str) -> None:
    _list_append(os.environ, "LD_PRELOAD", so)


def apply() -> None:
    sess = find_session()
    if sess.get("inject") != "1":
        return
    so = sess.get("TUXGT_LAUNCHER_SO", "").strip()
    for k, v in sess.items():
        if k in SKIP:
            continue
        _setenv(k, v)
    for p in (
        sess.get("TUXGT_GAME_DIR", ""),
        sess.get("TUXGT_DEPOT", ""),
    ):
        _pv_rw_add(p)
    ini = sess.get("TUXGT_LAUNCHER_INI", "")
    if ini:
        _pv_rw_add(str(Path(ini).parent))
    if so:
        _pv_rw_add(str(Path(so).parent))
        _preload_append(so)
