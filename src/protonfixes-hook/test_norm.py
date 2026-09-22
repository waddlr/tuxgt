"""SLOP T03 §1.4 golden exe-key vectors for tuxgt_apply._norm.

Mirrors the Rust contract in tuxgt-core session/tests_1.rs
(`canonical_key_folds_separators_case_and_trim`) and the shell assertions in
src/launcher/test/test_norm_path.sh. Keep the three in sync so drift fails
loudly. Run: `python3 test_norm.py` (stdlib unittest only). Not shipped:
both install paths (Makefile, userland/install.rs) name hook files
explicitly.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from tuxgt_apply import _norm

VECTORS = [
    # Pre-existing Rust cases (kept verbatim).
    ("C:\\Games\\SkyrimSE.EXE ", "c:/games/skyrimse.exe"),
    ("/PFX/Skyrim//", "/pfx/skyrim"),
    ("  /a / ", "/a"),
    ("", ""),
    # Backslash fold, lowercase, whitespace trim.
    ("a\\b\\c", "a/b/c"),
    ("ABC.EXE", "abc.exe"),
    ("\t /X/ ", "/x"),
    # Trailing slashes trim; leading slashes stay.
    ("foo///", "foo"),
    ("///a", "///a"),
    ("/", ""),
    ("   ", ""),
    # Combined: all folds at once.
    ("  C:\\GAMES\\Foo.EXE//  ", "c:/games/foo.exe"),
]


class NormTest(unittest.TestCase):
    def test_golden_vectors(self):
        for raw, want in VECTORS:
            with self.subTest(raw=raw):
                self.assertEqual(_norm(raw), want)


if __name__ == "__main__":
    unittest.main(verbosity=2)
