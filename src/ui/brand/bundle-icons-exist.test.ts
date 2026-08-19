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
const SOURCE = resolve(ROOT, 'docs', 'branding', 'loadout-icon.svg');

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

  it('keeps the built icon no older than the drawing it came from', () => {
    const icns = paths.find((one) => one.endsWith('.icns'));
    expect(icns, 'no .icns among the declared paths').toBeDefined();
    const built = resolve(ROOT, 'src-tauri', icns ?? '');
    expect(existsSync(built), 'the declared .icns is not on disk').toBe(true);
    expect(existsSync(SOURCE), 'the source drawing is not on disk').toBe(true);
    expect(
      statSync(built).mtimeMs,
      'the built icon is older than the drawing it comes from, so it is the icon of a PREVIOUS ' +
        'version of the mark — and it looks exactly like a fresh one. Run scripts/icons.sh.',
    ).toBeGreaterThanOrEqual(statSync(SOURCE).mtimeMs);
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
