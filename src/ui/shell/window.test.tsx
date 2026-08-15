/* Kryterium 1 dla T-01: okno otwiera się z czystym chrome, a adres dev wskazuje serwer,
 * który naprawdę stawiamy.
 *
 * Trzy asercje niżej wiążą DWA pliki albo DWIE wartości naraz i tylko one odróżniają poprawne
 * okno od okna, które wygląda tak samo, a jest zepsute:
 *
 *   `hiddenTitle === true`      — samo `titleBarStyle: "Overlay"` rysuje tytuł systemowy NA treści
 *   port dev == port z vite     — przykład Tauri daje 1420, Vite stoi na 5273, `npm run app` daje
 *                                 białe okno i nikt nie wie dlaczego
 *   odstęp >= x + 68            — bez tego przełącznik sekcji leży pod światłami
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { CHROME_INSET_LEFT, TITLEBAR_HEIGHT, TitleBar } from './titlebar';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Najwęższe wspierane okno, DESIGN.md §9. */
const NARROWEST = 1100;
/** Sufit chrome nad pierwszą treścią, ARCHITECTURE.md §7. */
const CHROME_CEILING = 96;
/** Trzy światła zajmują ~52 px, plus `--s-4` odstępu. */
const LIGHTS_PLUS_GAP = 68;

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

function fileJson(path: string): unknown {
  const raw = fileText(path);
  if (raw.trim() === '') return undefined;
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return undefined;
  }
}

function at(value: unknown, ...path: readonly string[]): unknown {
  let cursor: unknown = value;
  for (const key of path) {
    if (typeof cursor !== 'object' || cursor === null) return undefined;
    cursor = (cursor as Record<string, unknown>)[key];
  }
  return cursor;
}

function shown(value: unknown): string {
  return value === undefined ? '<nothing there>' : JSON.stringify(value);
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

const conf = fileJson(resolve(ROOT, 'src-tauri', 'tauri.conf.json'));
const viteText = fileText(resolve(ROOT, 'vite.config.ts'));
const windows = at(conf, 'app', 'windows');
const only = Array.isArray(windows) ? (windows[0] as unknown) : undefined;
const markup = renderToStaticMarkup(<TitleBar />);

describe('the window opens with clean chrome and the dev address is the one we serve', () => {
  it('declares exactly one window', () => {
    expect(
      Array.isArray(windows) ? windows.length : -1,
      'app.windows in tauri.conf.json has to hold exactly one entry, because everything else ' +
        'here — the permissions file, the drag area, the lights — points at that one window; ' +
        'the file says: ' + shown(windows),
    ).toBe(1);
  });

  it('gives that window the label a permissions file can point at', () => {
    expect(
      at(only, 'label'),
      'the window label has to be main, and it has to match the windows field of every file in ' +
        'src-tauri/capabilities/ — permissions naming a window that does not exist refuse every ' +
        'call from the webview; the file says: ' + shown(at(only, 'label')),
    ).toBe('main');
  });

  it('overlays the bar and hides the system title', () => {
    expect(
      at(only, 'titleBarStyle'),
      'titleBarStyle has to be Overlay so our own bar owns the top of the window; the file ' +
        'says: ' + shown(at(only, 'titleBarStyle')),
    ).toBe('Overlay');
    expect(
      at(only, 'hiddenTitle'),
      'hiddenTitle has to be true. Overlay alone still draws the system title, and it draws it ' +
        'ON TOP of our content — this is the half of the recipe that looks fine in a screenshot ' +
        'until there is text under it; the file says: ' + shown(at(only, 'hiddenTitle')),
    ).toBe(true);
  });

  it('places the lights by hand, with numbers', () => {
    const x = at(only, 'trafficLightPosition', 'x');
    const y = at(only, 'trafficLightPosition', 'y');
    expect(
      typeof x === 'number' && Number.isFinite(x),
      'trafficLightPosition.x has to be a number — the left inset of the bar is computed from ' +
        'it, so a missing value silently becomes a bar with no room for the lights; the file ' +
        'says: ' + shown(x),
    ).toBe(true);
    expect(
      typeof y === 'number' && Number.isFinite(y),
      'trafficLightPosition.y has to be a number; the file says: ' + shown(y),
    ).toBe(true);
  });

  it('keeps the narrowest supported width at 1100 or wider', () => {
    const minWidth = at(only, 'minWidth');
    expect(
      typeof minWidth === 'number' ? minWidth : -1,
      'minWidth has to be at least ' +
        String(NARROWEST) +
        ', the narrowest width DESIGN.md §9 supports; the file says: ' +
        shown(minWidth),
    ).toBeGreaterThanOrEqual(NARROWEST);
  });

  it('points the dev address at the port the front end really listens on', () => {
    const declared = /(?:^|\s)const\s+PORT\s*=\s*(\d+)/m.exec(viteText)?.[1];
    const devUrl = at(conf, 'build', 'devUrl');
    const used = typeof devUrl === 'string' ? /:(\d+)/.exec(devUrl)?.[1] : undefined;
    expect(
      declared,
      'vite.config.ts has to keep its port in a plain `const PORT = <number>`, because that is ' +
        'the only value this comparison can read it from',
    ).toBeDefined();
    expect(
      used,
      'build.devUrl in tauri.conf.json has to use the same port vite.config.ts serves on. The ' +
        'Tauri starter says 1420 and ours says ' +
        shown(declared) +
        '; take the starter value and npm run app opens a blank window with no error anywhere. ' +
        'devUrl says: ' + shown(devUrl),
    ).toBe(declared);
  });

  it('draws one bar and one drag area, under the chrome ceiling', () => {
    expect(
      occurrences(markup, 'data-tauri-drag-region'),
      'the bar has to carry exactly one drag area: none and the window cannot be moved, two and ' +
        'a click meant for a control drags the window instead',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-chrome'),
      'exactly one bar sits above the first content. A second one settles the density ceiling ' +
        'after the fact, which is how poprzedni prototyp reached 149 px of chrome on every screen',
    ).toBe(1);
    expect(
      TITLEBAR_HEIGHT,
      'TITLEBAR_HEIGHT has to stay at or under ' +
        String(CHROME_CEILING) +
        ' px, the ceiling ARCHITECTURE.md §7 fixed before this screen existed',
    ).toBeLessThanOrEqual(CHROME_CEILING);
  });

  it('leaves the lights their room before the first control', () => {
    const x = at(only, 'trafficLightPosition', 'x');
    const room = typeof x === 'number' ? x + LIGHTS_PLUS_GAP : Number.POSITIVE_INFINITY;
    expect(
      CHROME_INSET_LEFT,
      'the left inset of the bar has to clear the lights: trafficLightPosition.x plus ' +
        String(LIGHTS_PLUS_GAP) +
        ' px, which is ' +
        String(room) +
        ' here. Below that the section switcher sits under the lights and cannot be clicked',
    ).toBeGreaterThanOrEqual(room);
    expect(
      occurrences(markup, 'padding-left:' + String(CHROME_INSET_LEFT) + 'px'),
      'the inset has to reach the rendered bar exactly once. A constant nobody applies is a ' +
        'number that agrees with this measurement and with nothing on screen',
    ).toBe(1);
  });
});
