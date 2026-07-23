# App icons

All icons derive from hand-authored SVG sources and are rendered on macOS with
`qlmanage` (WebKit/CoreSVG) + `sips`/`iconutil` — no ImageMagick required.

## Sources
- `icon.svg` — squircle (rounded) art for **macOS** (`.icns`) and **Windows** (`.ico`).
- `icon-square.svg` — full-bleed variant (no rounded corners) for **iOS/Android**,
  which apply their own corner mask.
- `icon-fg.svg` / `icon-bg.svg` — Android adaptive-icon foreground (rocket) and
  background (gradient + sparkles) layers.

## Generated artifacts
- `icon.icns` — macOS bundle icon (referenced from `Dioxus.toml` → `[bundle].icon`).
- `icon.ico` — Windows multi-size icon (16/32/48/64/128/256, PNG-compressed).
- `icon-1024.png` — 1024px master (squircle).
- `platform/ios/AppIcon.appiconset/` — drop into the Xcode asset catalog.
- `platform/android/res/mipmap-*/` — drop into `android/app/src/main/res/`
  (legacy `ic_launcher[_round].png` + adaptive `ic_launcher_[foreground|background].png`
  + `mipmap-anydpi-v26/*.xml`).

## Regenerate
```sh
cd app/assets
# macOS .icns
qlmanage -t -s 1024 -o . icon.svg && mv icon.svg.png icon-1024.png
mkdir icon.iconset
for s in 16 32 128 256 512; do
  sips -z $s $s icon-1024.png --out icon.iconset/icon_${s}x${s}.png
  sips -z $((s*2)) $((s*2)) icon-1024.png --out icon.iconset/icon_${s}x${s}@2x.png
done
iconutil -c icns icon.iconset -o icon.icns && rm -rf icon.iconset
```
The Windows `.ico` and iOS/Android sets are produced by the same
`qlmanage` + `sips` steps (see git history for the exact packer commands).
