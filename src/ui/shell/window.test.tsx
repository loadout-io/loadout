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
import { CHROME_INSET_TOP, NAV_WIDTH, PANE_GAP, SideNav } from './titlebar';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');

/** Najwęższe wspierane okno, DESIGN.md §9. */
const NARROWEST = 1100;
/** Sufit chrome nad pierwszą treścią, ARCHITECTURE.md §7. */
const CHROME_CEILING = 96;
/** Trzy światła zajmują ~52 px, plus `--s-4` odstępu. */
/* Wysokość trzech świateł plus odstęp, licząc od `trafficLightPosition.y`. Menu stoi z boku,
 * więc światła trzeba minąć W PIONIE, nie w poziomie: 20 px świateł + 8 px odstępu. */
const LIGHTS_PLUS_GAP = 28;

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
const markup = renderToStaticMarkup(<SideNav />);

describe('the window opens with clean chrome and the dev address is the one we serve', () => {
  it('declares exactly one window', () => {
    expect(
      Array.isArray(windows) ? windows.length : -1,
      'app.windows in tauri.conf.json has to hold exactly one entry, because everything else ' +
        'here — the permissions file, the drag area, the lights — points at that one window; ' +
        'the file says: ' +
        shown(windows),
    ).toBe(1);
  });

  it('gives that window the label a permissions file can point at', () => {
    expect(
      at(only, 'label'),
      'the window label has to be main, and it has to match the windows field of every file in ' +
        'src-tauri/capabilities/ — permissions naming a window that does not exist refuse every ' +
        'call from the webview; the file says: ' +
        shown(at(only, 'label')),
    ).toBe('main');
  });

  it('overlays the bar and hides the system title', () => {
    expect(
      at(only, 'titleBarStyle'),
      'titleBarStyle has to be Overlay so our own bar owns the top of the window; the file ' +
        'says: ' +
        shown(at(only, 'titleBarStyle')),
    ).toBe('Overlay');
    expect(
      at(only, 'hiddenTitle'),
      'hiddenTitle has to be true. Overlay alone still draws the system title, and it draws it ' +
        'ON TOP of our content — this is the half of the recipe that looks fine in a screenshot ' +
        'until there is text under it; the file says: ' +
        shown(at(only, 'hiddenTitle')),
    ).toBe(true);
  });

  it('places the lights by hand, with numbers', () => {
    const x = at(only, 'trafficLightPosition', 'x');
    const y = at(only, 'trafficLightPosition', 'y');
    expect(
      typeof x === 'number' && Number.isFinite(x),
      'trafficLightPosition.x has to be a number — the left inset of the bar is computed from ' +
        'it, so a missing value silently becomes a bar with no room for the lights; the file ' +
        'says: ' +
        shown(x),
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
        'devUrl says: ' +
        shown(devUrl),
    ).toBe(declared);
  });

  /* 2026-08-17 — ZMIERZONE NA PRAWDZIWYM OKNIE, nie wydedukowane.
   *
   * Test wyżej pilnuje PORTU i jego komunikat obiecuje, że dzięki temu `npm run app` nie da
   * białego okna. Białe okno ma jednak drugą przyczynę, której tamta asercja nie widziała:
   * `host: false` w `vite.config.ts` każe Node'owi związać „localhost", a to na macOS
   * rozwiązuje się do `::1` — czyli serwer stoi WYŁĄCZNIE na IPv6 (`lsof`: `[::1]:5273`).
   * WKWebView pyta o IPv4, połączenie nie dochodzi, okno jest białe i NIGDZIE nie ma błędu:
   * strona nigdy nie zaczęła się ładować, więc nie ma czego zalogować.
   *
   * Dlatego adres jest tu sprawdzany W CAŁOŚCI, a nie po samym porcie: dwa pliki muszą mówić
   * o tym samym gnieździe, a nie o tej samej liczbie. */
  it('binds the dev server to an address the webview can actually reach', () => {
    const host = /(?:^|\s)host:\s*'([^']+)'/m.exec(viteText)?.[1];
    expect(
      host,
      'vite.config.ts has to bind an explicit loopback literal. `host: false` binds ' +
        '"localhost", which resolves to ::1 on macOS, so the server ends up IPv6-only and the ' +
        'webview asking for IPv4 gets a blank window with no error anywhere',
    ).toBe('127.0.0.1');

    const devUrl = at(conf, 'build', 'devUrl');
    const urlHost = typeof devUrl === 'string' ? /\/\/([^:/]+)/.exec(devUrl)?.[1] : undefined;
    expect(
      urlHost,
      'build.devUrl has to name the SAME host vite binds, and name it literally: `localhost` ' +
        'there re-introduces name resolution between two files that already agree. devUrl says: ' +
        shown(devUrl),
    ).toBe(host);
  });

  it('draws one nav and one drag area, and spends nothing from the chrome ceiling', () => {
    expect(
      occurrences(markup, 'data-tauri-drag-region'),
      'the nav has to carry exactly one drag area: none and the window cannot be moved, two and ' +
        'a click meant for a control drags the window instead',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-chrome'),
      'exactly one navigation metaphor. A second one settles the density ceiling after the ' +
        'fact, which is how poprzedni prototyp reached 149 px of chrome on every screen',
    ).toBe(1);
    /* 2026-08-17 — ta asercja mierzyła `TITLEBAR_HEIGHT <= 96` i była ZIELONA przy 138 px
     * realnego chrome, bo mierzyła jeden pasek z trzech: karty (34) i pasek loadoutu (56) też
     * stoją nad treścią, a ich nie widziała. Menu przeniesione do boku wnosi do tego sufitu
     * ZERO — i to jest teraz to, czego pilnujemy, zamiast liczby, która zgadzała się sama
     * ze sobą. Wysokość paska bocznego jest nieograniczona z definicji: to kolumna. */
    expect(
      occurrences(markup, 'height:'),
      'the side nav must not declare a height at all: a column that fixes its own height is a ' +
        'bar in disguise, and a bar above content spends the ' +
        String(CHROME_CEILING) +
        ' px ceiling ARCHITECTURE.md §7 fixed before this screen existed',
    ).toBe(0);
    expect(
      NAV_WIDTH,
      'the nav is a column beside the content, so it has a width and it comes from the mockup',
    ).toBeGreaterThan(0);
  });

  it('leaves the lights their room above the brand', () => {
    const y = at(only, 'trafficLightPosition', 'y');
    const room = typeof y === 'number' ? y + LIGHTS_PLUS_GAP : Number.POSITIVE_INFINITY;
    /* DWA UKLADY WSPOLRZEDNYCH, poprawione w T-46. `CHROME_INSET_TOP` jest odstepem LOKALNYM
     * dla kartki nawigacji, a wymog swiatel jest GLOBALNY dla okna. Dopoki kartka zaczynala sie
     * w punkcie (0,0) okna, te dwie liczby byly tym samym i asercja na samym `CHROME_INSET_TOP`
     * dzialala. Odkad kartka PLYWA o `PANE_GAP` nizej, roznia sie dokladnie o ten odstep —
     * i to on jest brakujacym skladnikiem, nie zmiana wymogu. */
    expect(
      PANE_GAP + CHROME_INSET_TOP,
      'the brand has to clear the lights measured in WINDOW coordinates: the pane floats ' +
        String(PANE_GAP) +
        ' px down and then insets its own content by ' +
        String(CHROME_INSET_TOP) +
        ', and the lights need trafficLightPosition.y plus ' +
        String(LIGHTS_PLUS_GAP) +
        ' px, which is ' +
        String(room) +
        ' here. Below that the brand sits under the lights and cannot be read.',
    ).toBeGreaterThanOrEqual(room);
    expect(
      occurrences(markup, 'padding-top:' + String(CHROME_INSET_TOP) + 'px'),
      'the inset has to reach the rendered nav exactly once. A constant nobody applies is a ' +
        'number that agrees with this measurement and with nothing on screen',
    ).toBe(1);
  });
});
