import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

/* AC-5 dla T-49: konfiguracja okna wskazuje na ikony, ktore ISTNIEJA.
 *
 * Do 2026-08-19 `src-tauri/tauri.conf.json` nie mial pola `bundle.icon` W OGOLE, wiec aplikacja
 * bundlowala sie z ikona domyslna, a w `src-tauri/icons/` lezal jeden osierocony `icon.png`,
 * ktorego nikt nie czytal.
 *
 * DLACZEGO PUNKT O WIEKU. `.icns` jest artefaktem, wiec plik zbudowany raz i nigdy nieodswiezony
 * jest ikona POPRZEDNIEJ wersji znaku — i wyglada dokladnie tak samo jak swiezy. To ta sama
 * rodzina wady co dokument projektowy rozjechany z kodem: dalej wyglada na prawde.
 *
 * DLACZEGO ROZMIARY Z NAGLOWKA PLIKU. Nazwa `128x128.png` jest napisem i nic nie kosztuje;
 * plik o 96 px pod ta nazwa macOS przeskaluje, a ikona przeskalowana z 96 na 128 jest mydlem.
 */

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const CONF = resolve(ROOT, 'src-tauri', 'tauri.conf.json');

/* WSZYSTKIE zrodla, nie jedno.
 *
 * Do 2026-08-19 swiezosc mierzona byla wzgledem `loadout-icon.svg`, a `.icns` powstaje z TRZECH
 * rysunkow: 16 px z rysunku 16, 16@2x i 32 z rysunku 32. Poprawienie jednego z tych dwoch bez
 * przebudowy zostawialo w Docku poprzednia wersje znaku dokladnie w tych rozmiarach, dla ktorych
 * te pliki istnieja — i kontrola byla zielona, bo porownywala sie z plikiem, ktory sie nie
 * zmienil. Skrypt jest tu razem z nimi: zmiana przepisu tez unieważnia zbudowany plik. */
const SOURCES = [
  resolve(ROOT, 'docs', 'branding', 'loadout-icon.svg'),
  resolve(ROOT, 'docs', 'branding', 'loadout-icon-32.svg'),
  resolve(ROOT, 'docs', 'branding', 'loadout-icon-16.svg'),
  resolve(ROOT, 'scripts', 'icons.sh'),
] as const;

const text = (path: string): string => (existsSync(path) ? readFileSync(path, 'utf8') : '');

/** Lista sciezek z `bundle.icon`, albo pusta. */
function declared(): readonly string[] {
  const raw = text(CONF);
  if (raw === '') return [];
  try {
    const conf = JSON.parse(raw) as { bundle?: { icon?: readonly string[] } };
    return conf.bundle?.icon ?? [];
  } catch {
    return [];
  }
}

/* Czlonkowie kontenera `.icns`: typ i dlugosc rekordu.
 *
 * Format jest prosty i dlatego da sie go tu przeczytac bez ani jednej zaleznosci: `icns`, dlugosc
 * calosci na czterech bajtach big-endian, a potem rekordy „cztery bajty typu, cztery dlugosci,
 * reszta to obraz". Sprawdzanie samej koncowki nazwy i niezerowego rozmiaru przepuszczalo plik
 * tekstowy nazwany `icon.icns` oraz — grozniej, bo wyglada dobrze — kontener z JEDNYM rozmiarem.
 * Zdanie nosne calej sekcji o ikonie brzmi „`.icns` jest ZESTAWEM, nie skalowaniem", wiec zestaw
 * trzeba policzyc. */
function icnsMembers(path: string): readonly (readonly [string, number])[] {
  if (!existsSync(path)) return [];
  const raw = readFileSync(path);
  if (raw.subarray(0, 4).toString('latin1') !== 'icns') return [];
  if (raw.readUInt32BE(4) !== raw.length) return [];
  const out: (readonly [string, number])[] = [];
  let at = 8;
  while (at + 8 <= raw.length) {
    const length = raw.readUInt32BE(at + 4);
    if (length < 8 || at + length > raw.length) break;
    out.push([raw.subarray(at, at + 4).toString('latin1'), length - 8] as const);
    at += length;
  }
  return out;
}

/** Szerokosc i wysokosc z naglowka IHDR pliku PNG. */
function pngSize(path: string): readonly [number, number] | null {
  if (!existsSync(path)) return null;
  const head = readFileSync(path).subarray(0, 33);
  if (head.subarray(1, 4).toString('ascii') !== 'PNG') return null;
  return [head.readUInt32BE(16), head.readUInt32BE(20)] as const;
}

describe('ikony aplikacji', () => {
  const paths = declared();

  it('declares a bundle icon at all', () => {
    expect(
      paths.length,
      'src-tauri/tauri.conf.json declares no bundle icon, so the app ships with the default one ' +
        'and every drawing in docs/branding is decoration nobody sees',
    ).toBeGreaterThan(0);
  });

  it('points every declared path at a file that is really there', () => {
    const missing = paths.filter((one) => {
      const full = resolve(ROOT, 'src-tauri', one);
      return !existsSync(full) || statSync(full).size === 0;
    });
    expect(
      missing,
      'these declared icon paths point at nothing: ' +
        JSON.stringify(missing) +
        '. A path naming a file that is not there is the same defect this project already had ' +
        'with the Inter typeface: it looks decided and it silently is not.',
    ).toEqual([]);
  });

  it('includes the one format macOS actually puts in the Dock', () => {
    expect(
      paths.some((one) => one.endsWith('.icns')),
      'no .icns is declared. macOS reads that one for the Dock and the app switcher; PNGs alone ' +
        'leave the most visible surface on the default icon.',
    ).toBe(true);
  });

  it('ships that .icns as a real SET of sizes, not one drawing under a matching name', () => {
    const icns = paths.find((one) => one.endsWith('.icns'));
    expect(icns, 'no .icns among the declared paths').toBeDefined();
    const members = icnsMembers(resolve(ROOT, 'src-tauri', icns ?? ''));
    expect(
      members.length,
      'the declared .icns does not read as an icns container at all — the magic bytes or the ' +
        'length in its header do not hold. A text file under that name passes an ends-with check ' +
        'and leaves the Dock on the default icon.',
    ).toBeGreaterThan(0);
    const images = members.filter(([type]) => type.startsWith('ic'));
    expect(
      images.map(([type]) => type),
      'the .icns carries fewer than ten sizes, so macOS scales one of them for the rest. Scaling ' +
        'is exactly what three separate drawings exist to avoid: at 32 px the full drawing turns ' +
        'the four edges into a blob.',
    ).toHaveLength(10);
    const empty = images.filter(([, size]) => size < 500);
    expect(
      empty.map(([type]) => type),
      'these members of the .icns are too small to be an image: ' +
        JSON.stringify(empty) +
        '. A container of ten placeholders counts as ten and shows as one.',
    ).toEqual([]);
  });

  it('keeps the built icon no older than ANY drawing it came from', () => {
    const icns = paths.find((one) => one.endsWith('.icns'));
    expect(icns, 'no .icns among the declared paths').toBeDefined();
    const built = resolve(ROOT, 'src-tauri', icns ?? '');
    expect(existsSync(built), 'the declared .icns is not on disk').toBe(true);
    const stale: string[] = [];
    for (const source of SOURCES) {
      expect(existsSync(source), source + ' is not on disk').toBe(true);
      if (statSync(built).mtimeMs < statSync(source).mtimeMs) stale.push(source);
    }
    expect(
      stale,
      'the built icon is older than these files it comes from: ' +
        JSON.stringify(stale.map((one) => one.slice(ROOT.length + 1))) +
        '. It is then the icon of a PREVIOUS version of the mark — and it looks exactly like a ' +
        'fresh one. Run scripts/icons.sh.',
    ).toEqual([]);
  });

  it('makes every PNG the size its own name states', () => {
    const wrong: string[] = [];
    for (const one of paths) {
      const size = /(\d+)x(\d+)(@2x)?\.png$/.exec(one);
      if (size === null) continue;
      const scale = size[3] === undefined ? 1 : 2;
      const want = Number(size[1]) * scale;
      const got = pngSize(resolve(ROOT, 'src-tauri', one));
      if (got === null) {
        wrong.push(one + ': not a readable PNG');
        continue;
      }
      if (got[0] !== want || got[1] !== want) {
        wrong.push(one + ': named ' + String(want) + ' and measures ' + got.join('x'));
      }
    }
    expect(
      wrong,
      'these icons are not the size their names state: ' +
        JSON.stringify(wrong) +
        '. The name is a string and costs nothing; a 96 px file under a 128 px name gets scaled ' +
        'by macOS, and a scaled icon is a blurred icon.',
    ).toEqual([]);
  });
});
