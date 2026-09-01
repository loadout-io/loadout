#!/usr/bin/env bash
# Buduje zestaw ikon aplikacji z TRZECH rysunkow SVG. Jedno zrodlo, jeden rysunek na rozmiar.
#
# DLACZEGO SKRYPT, A NIE RECZNE PLIKI. PNG i `.icns` sa artefaktami, a nie projektem: gdyby
# lezaly w repo bez generatora, drugi rysunek tej samej rzeczy powstalby przy pierwszej zmianie
# znaku i nikt by sie o tym nie dowiedzial (niezmiennik 13). Zrodlem prawdy sa pliki
# `docs/branding/loadout-icon*.svg`, a `src-tauri/icons/` jest z nich wyliczane.
#
# DLACZEGO CHROMIUM, A NIE `qlmanage`. Do 2026-09-01 rasteryzowal tu `qlmanage -t`, czyli
# generator MINIATUR Quick Looka -- a on sklada obraz na NIEPRZEZROCZYSTEJ BIELI. Kazda z czterech
# ikon wychodzila wiec z bialymi rogami `(255,255,255,255)` poza zaokragleniem squircle'a, widocznymi
# wszedzie, gdzie tlo nie jest biale: w Docku, na karcie repozytorium, w README.
#
# Wlasciciel zglaszal "biale elementy" TRZY RAZY (2026-08-19, 08-22, 09-01). Pierwsze dwie naprawy
# szukaly przyczyny w BARWIE TEMATU -- gaszenie znaku, potem przerysowanie go na jedna forme -- bo
# nikt nie zmierzyl piksela w rogu. Barwa nigdy nie byla przyczyna; rasteryzator byl.
#
# Chromium renderuje SVG z prawdziwym kanalem alfa i w DOKLADNYM rozmiarze, wiec znika przy okazji
# druga proteza: renderowanie z zapasem i schodzenie `sips`-em, bo miniatura bywala mniejsza od
# zadanej. Nie jest to nowa zaleznosc -- `@playwright/test` z Chromium stoi w tym repo dla e2e.
#
#   node + chromium  rasteryzuje SVG do PNG z alfa, w zadanym rozmiarze
#   iconutil         pakuje katalog `.iconset` w `.icns`
#
# KTORY RYSUNEK DO KTOREGO ROZMIARU. Przy 32 i 16 px rysunek pelny sie mydli: cztery krawedzie
# po 38 jednostek zlewaja sie w plame, a sheen i krawedz wewnetrzna operuja na 0,1 piksela.
# Dlatego 16 i 32 maja wlasne pliki, a `.icns` jest ZESTAWEM, nie skalowaniem.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

for tool in node iconutil; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool is not on PATH" >&2; exit 2; }
done
[ -d node_modules/@playwright/test ] || { echo "run npm install first -- the renderer is chromium" >&2; exit 2; }

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

# Rysownik. Mieszka TUTAJ, w przepisie, a nie w osobnym pliku: kryterium swiezosci
# (`src/ui/brand/bundle-icons-exist.test.ts`) uniewaznia zbudowane ikony, gdy zmieni sie
# `scripts/icons.sh`. Drugi plik z przepisem byloby drugim zrodlem prawdy, ktorego to kryterium
# NIE widzi -- czyli dokladnie ta wada, przed ktora ono stoi (niezmiennik 13).
read -r -d '' RENDERER <<'RENDERER' || true
import { readFileSync } from 'node:fs';
import { chromium } from '@playwright/test';

const size = Number(process.env.ICON_PX);
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: size, height: size } });
// `omitBackground` daje przezroczystosc tylko wtedy, gdy strona jej nie zamaluje, wiec tlo
// dokumentu musi byc jawnie przezroczyste. Rozmiar bierze CSS, a nie atrybuty rysunku: viewBox
// skaluje sie sam, wiec jeden plik obsluguje kazdy rozmiar bez przeskalowywania bitmapy.
await page.setContent(
  `<style>html,body{margin:0;padding:0;background:transparent}` +
    `svg{display:block;width:${size}px;height:${size}px}</style>` +
    readFileSync(process.env.ICON_SVG, 'utf8'),
);
await page.screenshot({
  path: process.env.ICON_OUT,
  omitBackground: true,
  clip: { x: 0, y: 0, width: size, height: size },
});
await browser.close();
RENDERER

# render <svg> <px> <out.png>
render() {
  # Rysownik idzie przez `-e`, a nie plikiem w katalogu tymczasowym: node szuka `@playwright/test`
  # idac w gore OD SKRYPTU, wiec plik poza repo nie widzi jego `node_modules`. Argumenty ida
  # srodowiskiem, bo `-e` przesuwa `process.argv`.
  ICON_SVG="$1" ICON_PX="$2" ICON_OUT="$3" node --input-type=module -e "$RENDERER"
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
