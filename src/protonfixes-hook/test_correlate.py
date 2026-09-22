"""Prefix-fallback vectors for tuxgt_apply.correlate (hook side).

Exact (prefix, exe) hits keep priority; an exe with no key of its own
resolves when its prefix section names exactly one rel (launcher-first
chains: MO2 outer, SKSE middle — the inner game exe inherits the env).
Contested prefixes, unknown prefixes, and missing conf files miss.
Run: `python3 test_correlate.py` (stdlib unittest only). Not shipped:
both install paths (Makefile, userland/install.rs) name hook files
explicitly.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from tuxgt_apply import apply, correlate, parse_session


def env(prefix: str, exe: str) -> dict[str, str]:
    return {"STEAM_COMPAT_DATA_PATH": prefix, "EXE": exe}


class CorrelateTest(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.data = Path(tmp.name)
        (self.data / "games" / "steam" / "mo2test").mkdir(parents=True)
        (self.data / "games" / "steam" / "mo2test" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        (self.data / "games" / "load-correlator.ini").write_text(
            "# tuxgt load-correlator\n"
            "\n"
            "[/pfx/mo2]\n"
            "c:/games/stock game/skyrimse.exe=steam/mo2test\n"
        )

    def test_exact_hit(self) -> None:
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\Stock Game\\SkyrimSE.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))

    def test_fallback_hit_single_rel(self) -> None:
        # SKSE middle link: no key of its own, single-rel prefix resolves.
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))

    def test_exact_beats_fallback(self) -> None:
        # Contested prefix, but the exe has its own key: exact wins.
        (self.data / "games" / "heroic_gog" / "abc").mkdir(parents=True)
        (self.data / "games" / "heroic_gog" / "abc" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/tool/tool.exe=heroic_gog/abc\n")
        conf = correlate(self.data, env("/pfx/mo2", "c:/games/tool/tool.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("heroic_gog/abc/tux-protonfixes.conf"))

    def test_contested_prefix_misses(self) -> None:
        (self.data / "games" / "heroic_gog" / "abc").mkdir(parents=True)
        (self.data / "games" / "heroic_gog" / "abc" / "tux-protonfixes.conf").write_text(
            "inject=1\n"
        )
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/tool/tool.exe=heroic_gog/abc\n")
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe")))

    def test_unknown_prefix_misses(self) -> None:
        self.assertIsNone(correlate(self.data, env("/pfx/unknown", "C:\\Games\\skse64_loader.exe")))

    def test_missing_conf_misses(self) -> None:
        # Rel resolves but its session file is gone: miss, not a bad path.
        (self.data / "games" / "steam" / "mo2test" / "tux-protonfixes.conf").unlink()
        self.assertIsNone(
            correlate(self.data, env("/pfx/mo2", "C:\\Games\\Stock Game\\SkyrimSE.exe"))
        )
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe")))

    def test_missing_keys_miss(self) -> None:
        self.assertIsNone(correlate(self.data, env("", "C:\\Games\\skse64_loader.exe")))
        self.assertIsNone(correlate(self.data, env("/pfx/mo2", "")))



    def test_empty_value_ignored(self) -> None:
        # Stray empty-valued key: ignored, single real rel still resolves.
        with open(self.data / "games" / "load-correlator.ini", "a") as f:
            f.write("c:/games/stray/stray.exe=\n")
        conf = correlate(self.data, env("/pfx/mo2", "C:\\Games\\skse64_loader.exe"))
        self.assertIsNotNone(conf)
        assert conf is not None
        self.assertTrue(str(conf).endswith("steam/mo2test/tux-protonfixes.conf"))


class ApplyPreloadTest(unittest.TestCase):
    def setUp(self) -> None:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.data = Path(tmp.name)
        lib = self.data / "lib"
        lib.mkdir()
        self.so32 = lib / "libtuxgt-launcher32.so"
        self.so64 = lib / "libtuxgt-launcher.so"
        self.so32.write_bytes(b"32")
        self.so64.write_bytes(b"64")
        rel = self.data / "games" / "heroic_gog" / "jc2"
        rel.mkdir(parents=True)
        (rel / "tux-protonfixes.conf").write_text(
            f"inject=1\nTUXGT_LAUNCHER_SO={self.so32}\n"
        )
        (self.data / "games" / "load-correlator.ini").write_text(
            "[/pfx/jc2]\nc:/games/justcause2.exe=heroic_gog/jc2\n"
        )
        keys = (
            "TUXGT_DATA",
            "STEAM_COMPAT_DATA_PATH",
            "EXE",
            "LD_PRELOAD",
            "PRESSURE_VESSEL_FILESYSTEMS_RW",
            "TUXGT_LAUNCHER_SO",
            "TUXGT_GAME_DIR",
            "TUXGT_DEPOT",
            "TUXGT_LAUNCHER_INI",
        )
        old = {k: os.environ.get(k) for k in keys}

        def restore() -> None:
            for k, v in old.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

        self.addCleanup(restore)
        os.environ["TUXGT_DATA"] = str(self.data)
        os.environ["STEAM_COMPAT_DATA_PATH"] = "/pfx/jc2"
        os.environ["EXE"] = "C:\\games\\justcause2.exe"
        for k in (
            "LD_PRELOAD",
            "PRESSURE_VESSEL_FILESYSTEMS_RW",
            "TUXGT_LAUNCHER_SO",
        ):
            os.environ.pop(k, None)

    def test_apply_preloads_32bit_primary_and_64bit_sibling(self) -> None:
        apply()
        preload = os.environ.get("LD_PRELOAD", "")
        self.assertIn(str(self.so32), preload)
        self.assertIn(str(self.so64), preload)
        self.assertEqual(os.environ.get("TUXGT_LAUNCHER_SO"), str(self.so32))
        self.assertIn(str(self.so32.parent), os.environ.get("PRESSURE_VESSEL_FILESYSTEMS_RW", ""))

    def test_apply_skips_missing_sibling(self) -> None:
        self.so64.unlink()
        apply()
        preload = os.environ.get("LD_PRELOAD", "")
        self.assertIn(str(self.so32), preload)
        self.assertNotIn("libtuxgt-launcher.so", preload.replace("libtuxgt-launcher32.so", ""))


class ParseSessionTest(unittest.TestCase):
    def test_quotes_and_legacy(self) -> None:
        text = """\
# tuxgt launch session
inject=1
FOO=bar
WINEDLLOVERRIDES="dxgi=n,b;d3d12=n,b"
BAR="hello world"
BAZ="a\\$b\\`c\\"d\\\\e"
NL="a
b"
SQ='it'\\''s'
LEGACY=d3dcompiler_47=n;dxgi=n,b
"""
        got = parse_session(text)
        self.assertEqual(got["inject"], "1")
        self.assertEqual(got["FOO"], "bar")
        self.assertEqual(got["WINEDLLOVERRIDES"], "dxgi=n,b;d3d12=n,b")
        self.assertEqual(got["BAR"], "hello world")
        self.assertEqual(got["BAZ"], "a$b`c\"d\\e")
        self.assertEqual(got["NL"], "a\nb")
        self.assertEqual(got["SQ"], "it's")
        self.assertEqual(got["LEGACY"], "d3dcompiler_47=n;dxgi=n,b")


if __name__ == "__main__":
    unittest.main(verbosity=2)
