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
    if not path.is_file():
        return {}
    return parse_session(path.read_text(encoding="utf-8", errors="replace"))


def parse_session(text: str) -> dict[str, str]:
    """Session assignments. A quoted value is one shell word (`=`, `;`, spaces, newlines)."""
    out: dict[str, str] = {}
    i = 0
    n = len(text)
    while i < n:
        if text[i] in "\n\r":
            i += 1
            continue
        if text[i] == "#":
            nl = text.find("\n", i)
            i = n if nl < 0 else nl + 1
            continue
        eq = text.find("=", i)
        nl = text.find("\n", i)
        if eq < 0 or (0 <= nl < eq):
            i = n if nl < 0 else nl + 1
            continue
        key = text[i:eq]
        if not key or any(c not in _KEY for c in key):
            i = n if nl < 0 else nl + 1
            continue
        i = eq + 1
        if i < n and text[i] == '"':
            val, i, ok = _dquote(text, i + 1)
        elif i < n and text[i] == "'":
            val, i, ok = _squote(text, i + 1)
        else:
            end = text.find("\n", i)
            if end < 0:
                val, i, ok = text[i:], n, True
            else:
                val, i, ok = text[i:end], end + 1, True
        if ok:
            out[key] = val
        else:
            break
    return out


_KEY = frozenset("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_")


def _trail(text: str, i: int) -> tuple[int, bool]:
    n = len(text)
    while i < n and text[i] in " \t":
        i += 1
    if i < n and text[i] == "\r":
        i += 1
    if i < n and text[i] == "\n":
        return i + 1, True
    return i, i >= n


def _dquote(text: str, i: int) -> tuple[str, int, bool]:
    n = len(text)
    out: list[str] = []
    while i < n:
        c = text[i]
        if c == "\\":
            if i + 1 >= n:
                return "", i, False
            out.append(text[i + 1])
            i += 2
            continue
        if c == '"':
            j, ok = _trail(text, i + 1)
            return "".join(out), j, ok
        out.append(c)
        i += 1
    return "", i, False


def _squote(text: str, i: int) -> tuple[str, int, bool]:
    n = len(text)
    out: list[str] = []
    while i < n:
        if text[i] == "'":
            if text.startswith("\\''", i + 1):
                out.append("'")
                i += 4
                continue
            j, ok = _trail(text, i + 1)
            return "".join(out), j, ok
        out.append(text[i])
        i += 1
    return "", i, False


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


def _companion_so(so: str) -> str:
    p = Path(so)
    name = p.name
    if name == "libtuxgt-launcher32.so":
        sib = p.with_name("libtuxgt-launcher.so")
    elif name == "libtuxgt-launcher.so":
        sib = p.with_name("libtuxgt-launcher32.so")
    else:
        return ""
    return str(sib) if sib.is_file() else ""


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
        _preload_append(_companion_so(so))
