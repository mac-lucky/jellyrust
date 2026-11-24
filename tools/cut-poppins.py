#!/usr/bin/env python3
"""Cut the shipped UI faces - `pkg/appfont.ttf` and `pkg/appfont-bold.ttf` - from Poppins.

    uv run --with fonttools python -I tools/cut-poppins.py DIR

DIR holds `Poppins-Regular.ttf` and `Poppins-SemiBold.ttf` from github.com/google/fonts `ofl/poppins`
(Poppins 4.004, pinned by sha256 below).

**Why SemiBold for the bold face.** Litefin ships Poppins Regular only, so every 600/700 weight the
household sees is Chromium's synthetic bold of the Regular outlines (`docs/litefin-look.md` 2.2).
SDL_ttf can only embolden at ppem/10 (`gfx/text.rs`), far heavier than Skia's ~1px at 32px, so a
real cut stands in. Measured on "Continue Watching" at 32px against `docs/litefin-ref/home.png`
(FreeType, light hinting): Regular + Skia fake bold 305px wide at 9.63 ink per column, SemiBold
313px at 9.65, Bold 319px at 11.14. SemiBold matches the synthetic weight and is the closer width.

This replaces `tools/cut-inter.py` as the recipe for the two files (Inter itself still ships, as
`appfont-inter*.ttf`: the subtitle faces and the UI chain's second link); the device fixes that
script documents still apply, and its code is reused here:

  1. **Proportional figures, as Litefin's.** This Poppins has no `tnum` and no `.tf` alternates
     ('1' = 0.320 em, '6' = 0.635 em), and Litefin draws it with those proportional digits, so the
     cut keeps them. A read-out that ticks holds still by drawing each digit in a fixed cell
     (`ui::widgets::draw_figures`), not by a frozen tabular cmap.
  2. **No legacy `kern` table, because there is nothing to flatten**: this Poppins carries no
     kerning at all (no GPOS `kern` feature), so SDL2_ttf draws it unkerned either way.
  3. **The music notes U+2669-U+266C**, drawn by cut-inter's `add_music_notes`.
  4. **No "PlxSubs" alias.** libass is handed the Inter faces, not these (`player/ass/fonts.rs`),
     and `appfont-inter-bold.ttf` carries the alias.
  5. **Every codepoint a `pkg/fonts/subs` face carries is removed** (Poppins' Devanagari):
     `gfx/text.rs` has no shaper, so unshaped Devanagari would be fluent-looking wrong text.
"""
import hashlib
import importlib.util
import json
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parent
sys.path.insert(0, str(TOOLS))

_spec = importlib.util.spec_from_file_location("cut_inter", TOOLS / "cut-inter.py")
cut_inter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cut_inter)

INPUTS = {
    "Poppins-Regular.ttf": "7e65201e9b79159e2300267cc885e16c8dcef2424cdfa09a29bfb0980a94a7ba",
    "Poppins-SemiBold.ttf": "d3bf1bdaf0550e83da9ac0b1d1d9fe6db086835a83aa28578e609a394b9a0286",
}
# The three codepoints `fonts/tests.rs` lets a Noto face share with the bold face.
SHARED_OK = {0x200C, 0x200D, 0x25CC}
POLISH = [0x105, 0x107, 0x119, 0x142, 0x144, 0xF3, 0x15B, 0x17A, 0x17C,
          0x104, 0x106, 0x118, 0x141, 0x143, 0xD3, 0x15A, 0x179, 0x17B]


def subs_codepoints() -> set:
    """Every codepoint any shipped subtitle face carries."""
    subs = REPO / "pkg" / "fonts" / "subs"
    faces = json.loads((subs / "manifest.json").read_text(encoding="utf-8"))["faces"]
    out = set()
    for face in faces:
        out.update(TTFont(subs / face["file"], lazy=True).getBestCmap())
    return out


def drop_codepoints(font: TTFont, drop: set) -> int:
    """Subset `font` to its own cmap minus `drop`, keeping names, glyph names and layout."""
    keep = set(font.getBestCmap()) - drop
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs, opts.name_languages, opts.name_legacy = ["*"], ["*"], True
    opts.glyph_names = opts.notdef_outline = opts.legacy_kern = True
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=keep)
    before = len(font.getBestCmap())
    sub.subset(font)
    return before - len(font.getBestCmap())


def check(path: Path, subs: set) -> None:
    """The properties the Rust gates and the device rely on, read back from the saved file."""
    f = TTFont(path)
    cmap = f.getBestCmap()
    assert len({f["hmtx"][cmap[0x30 + i]][0] for i in range(10)}) > 1, "digits are frozen tabular"
    assert all(cp in cmap for cp in range(0x2669, 0x266D)), "music notes missing"
    assert all(cp in cmap for cp in POLISH), "Polish letters missing"
    assert not (set(cmap) & subs) - SHARED_OK, "still shares codepoints with a subtitle face"
    assert "DSIG" not in f, "stale DSIG"
    names = {(r.nameID, r.langID, r.toUnicode()) for r in f["name"].names if r.platformID == 3}
    assert (1, 0x407, "PlxSubs") not in names, "PlxSubs belongs to appfont-inter-bold.ttf"


def main() -> int:
    if len(sys.argv) != 2:
        return print(__doc__) or 2
    src = Path(sys.argv[1])
    for name, sha in INPUTS.items():
        got = hashlib.sha256((src / name).read_bytes()).hexdigest()
        if got != sha:
            raise SystemExit(f"{name}: sha256 {got}, pinned {sha}")
    subs = subs_codepoints()
    for name, out in (("Poppins-Regular.ttf", "appfont.ttf"),
                      ("Poppins-SemiBold.ttf", "appfont-bold.ttf")):
        f = TTFont(src / name, recalcTimestamp=False)  # same inputs, same bytes
        dropped = drop_codepoints(f, subs - SHARED_OK)
        notes = cut_inter.add_music_notes(f)
        if "DSIG" in f:
            del f["DSIG"]
        dst = REPO / "pkg" / out
        f.save(dst)
        check(dst, subs)
        print(f"  {out:16s} {name} | {dropped} subtitle-face codepoints dropped | {notes} note mappings "
              f"| {len(f.getBestCmap())} codepoints | {dst.stat().st_size // 1024} KB")
    print("\nRe-run tools/font-hint-audit.py - these are new files and the ladder must be re-confirmed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
