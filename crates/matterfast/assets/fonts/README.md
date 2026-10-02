# Bundled fonts

The application ships its own fonts instead of relying on what the desktop has
installed, because the text renderer in this GPUI snapshot cannot use two things
most current distributions now ship by default:

* **variable fonts** — a variable UI font (Adwaita Sans, Noto Sans on Fedora)
  is loaded as one regular face, so nothing drawn bold or semibold actually is;
* **COLRv1 emoji** — Fedora's Noto Color Emoji is COLRv1, which the rasteriser
  does not draw at all: every emoji came out as a blank space.

## Inter

`Inter-*.ttf` are the static faces from the Inter 4.1 release
(<https://github.com/rsms/inter>), unmodified. SIL Open Font License 1.1 — see
`Inter-LICENSE.txt`.

## Emoji

`NotoColorEmoji-Twemoji.ttf` is **Twemoji Mozilla 0.7.0**
(<https://github.com/mozilla/twemoji-colr>), a COLRv0 font the rasteriser can
draw. The glyphs are Twitter's Twemoji graphics, CC-BY 4.0; the font build is
Mozilla's, Apache 2.0 — see `Twemoji-LICENSE.md`.

It is modified in exactly one way: its `name` table says family
`Noto Color Emoji`, PostScript name `NotoColorEmoji`. That is not a claim to be
Noto. GPUI draws a glyph in colour only when the font's PostScript name is
literally `NotoColorEmoji`, and the fallback list that finds an emoji font at
all looks for the family `Noto Color Emoji`; under any other name this font is
found by neither. `src/fonts.rs` explains how it takes the system font's place.

To rebuild it from the upstream file:

```python
from fontTools.ttLib import TTFont
f = TTFont("Twemoji.Mozilla.ttf")
for n in f["name"].names:
    if n.nameID in (1, 4, 16): n.string = "Noto Color Emoji"
    if n.nameID == 6: n.string = "NotoColorEmoji"
    if n.nameID == 3: n.string = "Twemoji Mozilla 0.7.0, renamed for Matterfast"
if "FFTM" in f: del f["FFTM"]
f.save("NotoColorEmoji-Twemoji.ttf")
```
