#!/usr/bin/env bash
# Buduje zestaw ikon aplikacji z TRZECH rysunkow SVG. Jedno zrodlo, jeden rysunek na rozmiar.
#
# DLACZEGO SKRYPT, A NIE RECZNE PLIKI. PNG i `.icns` sa artefaktami, a nie projektem: gdyby
# lezaly w repo bez generatora, drugi rysunek tej samej rzeczy powstalby przy pierwszej zmianie
# znaku i nikt by sie o tym nie dowiedzial (niezmiennik 13). Zrodlem prawdy sa pliki
# `docs/branding/loadout-icon*.svg`, a `src-tauri/icons/` jest z nich wyliczane.
#
# DLACZEGO TE TRZY NARZEDZIA. `rsvg-convert`, Inkscape i ImageMagick nie sa w tym systemie
# (sprawdzone 2026-08-19). Sa natomiast trzy narzedzia Apple i one wystarczaja:
#   qlmanage -t   rasteryzuje SVG do PNG
#   sips          doprowadza do DOKLADNEGO rozmiaru (miniatura qlmanage bywa mniejsza)
#   iconutil      pakuje katalog `.iconset` w `.icns`
#
# KTORY RYSUNEK DO KTOREGO ROZMIARU. Przy 32 i 16 px rysunek pelny sie mydli: cztery krawedzie
# po 38 jednostek zlewaja sie w plame, a sheen i krawedz wewnetrzna operuja na 0,1 piksela.
# Dlatego 16 i 32 maja wlasne pliki, a `.icns` jest ZESTAWEM, nie skalowaniem.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

for tool in qlmanage sips iconutil; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool is not on PATH" >&2; exit 2; }
done

SRC_FULL="docs/branding/loadout-icon.svg"
SRC_32="docs/branding/loadout-icon-32.svg"
SRC_16="docs/branding/loadout-icon-16.svg"
for f in "$SRC_FULL" "$SRC_32" "$SRC_16"; do
  [ -f "$f" ] || { echo "$f is missing -- the drawings are the source, not the output" >&2; exit 2; }
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
SET="$WORK/loadout.iconset"
mkdir -p "$SET" src-tauri/icons

# render <svg> <px> <out.png>
render() {
  local svg="$1" px="$2" out="$3"
  rm -rf "$WORK/ql"; mkdir -p "$WORK/ql"
  # Renderujemy z zapasem i dopiero `sips` schodzi do docelowego rozmiaru: miniatura Quick Looka
  # bywa mniejsza od zadanej, a ikona o 511 px zamiast 512 jest ikona, ktora macOS przeskaluje.
  qlmanage -t -s "$((px * 2))" -o "$WORK/ql" "$svg" >/dev/null 2>&1 || true
  local made
  made="$(find "$WORK/ql" -name '*.png' -print -quit)"
  [ -n "$made" ] || { echo "qlmanage produced no PNG for $svg" >&2; exit 1; }
  sips -z "$px" "$px" "$made" --out "$out" >/dev/null
}

# Zestaw `.icns`: kazdy rozmiar z rysunku, ktory dla niego powstal.
render "$SRC_16"   16  "$SET/icon_16x16.png"
render "$SRC_32"   32  "$SET/icon_16x16@2x.png"
render "$SRC_32"   32  "$SET/icon_32x32.png"
render "$SRC_FULL" 64  "$SET/icon_32x32@2x.png"
render "$SRC_FULL" 128 "$SET/icon_128x128.png"
render "$SRC_FULL" 256 "$SET/icon_128x128@2x.png"
render "$SRC_FULL" 256 "$SET/icon_256x256.png"
render "$SRC_FULL" 512 "$SET/icon_256x256@2x.png"
render "$SRC_FULL" 512 "$SET/icon_512x512.png"
render "$SRC_FULL" 1024 "$SET/icon_512x512@2x.png"
iconutil -c icns "$SET" -o src-tauri/icons/icon.icns

# PNG-i, ktore czyta `tauri.conf.json`.
render "$SRC_32"   32  src-tauri/icons/32x32.png
render "$SRC_FULL" 128 src-tauri/icons/128x128.png
render "$SRC_FULL" 256 "src-tauri/icons/128x128@2x.png"
render "$SRC_FULL" 1024 src-tauri/icons/icon.png

echo "icons: 1 icns + 4 png from 3 drawings"
ls -la src-tauri/icons/
