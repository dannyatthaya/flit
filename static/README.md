# Flit — App Icon Assets

**Mark:** Soundwave · **Theme:** Ink + Mint
**Colors:** Tile `#15171A` · Wave `#2EE6A0`

## Contents

```
flit.ico                 Windows app icon (16,32,48,64,128,256)
flit.icns                macOS app icon (16–1024)
svg/
  flit-icon-tile.svg     Full-color tile (vector, master)
  flit-mark.svg          Wave mark, green, transparent (vector)
  flit-tray-mono.svg     Tray glyph, black template (vector)
png/
  tile/                  Full-color app tile — 16,32,64,128,256,512,1024
  mark-square/           Green wave, transparent, square — 32–512 (favicon/avatar)
  mark-wide/             Green wave, transparent, wide lockup — 256,512,1024
  tray-white/            Monochrome white — for dark menu bars — 16,18,32,36,44,64,88
  tray-black/            Monochrome black — for light menu bars — 16,18,32,36,44,64,88
```

## Usage notes
- **App tile:** use `flit.ico` (Windows) / `flit.icns` (macOS), or the `png/tile` set.
- **Favicon:** `png/tile/flit-tile-32.png` + `svg/flit-icon-tile.svg`, or the green `mark-square` set for a transparent favicon.
- **System tray:** ship the size matching the OS (16/18 @1x, 32/36 @2x). Use white on dark bars, black on light. On macOS, supply the black version as a template image so the OS tints it automatically.
- SVGs are the masters — rescale from these for any size not listed.
