/**
 * KOLEKTOR gęstości: mierzy DOM zbudowanej aplikacji i zapisuje zrzut dla sędziego.
 *
 * Podział, którego ten plik jest drugą połową, stoi w nagłówku `scripts/density-audit.mjs`
 * i w `checks/density.sh`: SĘDZIA jest czystą funkcją nad JSON-em i to on ma kryteria;
 * KOLEKTOR biegnie w przeglądarce i kryterium akceptacji mieć NIE MOŻE. `Failed to launch`
 * i `Executable doesn't exist` stoją na liście `NOT_A_REAL_RED` w bramce, więc kryterium
 * wymagające Chromium dawałoby w `before` czerwień, którą bramka odrzuca, a w `full` zieleń,
 * która nic nie znaczy. Repo źródłowe scertyfikowało tak siedem kryteriów na przeglądarce,
 * która nie startowała [03 §4.1].
 *
 * DWIE SZEROKOŚCI, bo sufit ma obowiązywać na obu: 1100 px to najwęższe wspierane okno
 * (`docs/design/DESIGN.md` §9), 1512 px to szerokość odniesienia z [03 §4.1]. Sędzia bierze
 * GORSZĄ z dwóch — pomiar chowający się za drugim meldowałby „pass" o ekranie, którego nikt
 * nie widział takim, jakim go zmierzono.
 *
 * CZTERY METRYKI Z SIEDMIU SĄ MECHANICZNE i tyle ten plik mierzy. Pozostałe trzy jadą
 * do `notMeasured` Z POWODEM — i to jest granica, nie wygoda. `checks/density.sh` w swoim
 * nagłówku nazywa wprost rzecz, której nie wolno zrobić: „zrzut z siedmioma metrykami
 * »niezmierzone, powód: kolektor nie biegł« — sędzia by to przepuścił, i byłaby to zieleń
 * kupiona za zdanie". Każdy powód niżej mówi więc, dlaczego TA metryka nie jest liczbą,
 * a nie dlaczego kolektor się nie postarał.
 *
 * ATRAPA `__TAURI_INTERNALS__` JEST TU WŁASNA I MNIEJSZA niż ta z `e2e/harness.ts`, celowo.
 * Tamta nagrywa wywołania i umie odpowiadać scenariuszami, bo tamte kryteria pytają, co
 * aplikacja WYSŁAŁA. Tutaj pytanie brzmi „ile jest na ekranie", więc jedyne, czego atrapa
 * musi dopilnować, to żeby ekran się w ogóle dorysował: front, który dostał `undefined` tam,
 * gdzie spodziewa się listy, przewraca się na własnym `for` i wtedy mierzylibyśmy pusty
 * dokument jako „bardzo rzadki ekran".
 *
 * Uruchomienie:  npm run build && node scripts/density-collect.mjs --out <plik.json>
 * Potem:         LOADOUT_DENSITY_SNAPSHOT=<plik.json> bash checks/density.sh
 */
import { createReadStream, existsSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize } from 'node:path';

import { chromium } from '@playwright/test';

const ROOT = new URL('..', import.meta.url).pathname;
const DIST = join(ROOT, 'dist');

/** Szerokości okna, przy których sufit ma obowiązywać. Wysokość jedna: mierzymy szerokość. */
const WIDTHS = [1100, 1512];
const HEIGHT = 900;

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.woff2': 'font/woff2',
  '.json': 'application/json; charset=utf-8',
};

/** Serwuje `dist/` — mierzymy ZBUDOWANĄ aplikację, nie ten sam kod przez dev server. */
function serveDist() {
  const server = createServer((request, response) => {
    const asked = (request.url ?? '/').split('?')[0];
    const relative = normalize(asked === '/' ? '/index.html' : asked).replace(/^(\.\.[/\\])+/, '');
    const file = join(DIST, relative);
    const target = existsSync(file) ? file : join(DIST, 'index.html');
    response.writeHead(200, {
      'content-type': TYPES[extname(target)] ?? 'application/octet-stream',
    });
    createReadStream(target).pipe(response);
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      resolve({ server, port: server.address().port });
    });
  });
}

/**
 * Odpowiada KSZTAŁTEM, nigdy treścią — z jednym wyjątkiem, i ten wyjątek jest treścią pomiaru.
 *
 * JEDEN WORKSPACE JEST CZĘŚCIĄ SCENY, NIE ZMYŚLENIEM (2026-08-30). Pierwsza wersja tego pliku
 * odpowiadała pustą listą na `list_workspaces` i zmierzyła **137 px** chrome przy suficie 96 —
 * po czym zgłosiłem właścicielowi naruszenie, którego w jego aplikacji nie ma. Te 44 px
 * różnicy to `[data-add-workspace]`: przycisk zapraszający do wskazania pierwszego folderu,
 * który stoi na ekranie WYŁĄCZNIE dopóki nie ma ani jednego zakresu, i znika na zawsze po
 * pierwszym wskazaniu. Zmierzone tego samego dnia, obie sceny:
 *
 *     bez workspace   chrome = 137 px   (zaproszenie na ekranie)
 *     z workspace     chrome =  93 px   (zaproszenia nie ma)
 *
 * Sufit z §7 mówi o widoku, w którym człowiek PRACUJE, a nie o ekranie pierwszego startu —
 * i to jest granica tego pomiaru, powiedziana wprost, bo pomiar z nieopisaną granicą jest
 * gorszy niż jego brak. Zaproszenie nie jest długiem gęstości: DESIGN §6 żąda go zamiast
 * komunikatu o braku danych, a kosztuje 36 px przez jedno kliknięcie w życiu instalacji.
 *
 * Jedno pole `folder` jest zmyślone i tylko ono. Żaden agent, skill, workflow ani bieg nie
 * jest udawany — wszystkie `list_*` dalej oddają pustą listę.
 */
function stubTauri() {
  const host = globalThis;
  const ONE_WORKSPACE = [{ id: '/density/scene', name: 'Density scene', folder: '/density/scene' }];

  /* JEDEN AGENT I JEDEN WORKFLOW SA CZESCIA SCENY Z DOKLADNIE TEGO SAMEGO POWODU, CO WORKSPACE
   * WYZEJ (2026-09-01). Kiedy pisano ten plik, pusta biblioteka dawala ekran biegu z pustymi
   * listami — czyli widok pracy. Od `b107bc1` pusta biblioteka daje EKRAN PIERWSZEGO STARTU:
   * powitanie, sciezke „0 of 3 done" i galerie gotowych agentow. Zmierzone tego dnia, obie sceny:
   *
   *     pusta biblioteka   textElements = 82   (powitanie na ekranie)
   *     jeden agent + jeden workflow   patrz `dist/density-snapshot.json`
   *
   * Reguła tego pliku jest juz zapisana wyzej i brzmi: sufit z §7 mowi o widoku, w ktorym
   * czlowiek PRACUJE, a nie o ekranie pierwszego startu. Powitanie jest tym drugim — stoi
   * dopoki nie ma ani jednego agenta i znika po pierwszym. Mierzenie go pod sufitem widoku
   * pracy jest ta sama pomylka, ktora ten plik juz raz naprawil przy zaproszeniu do folderu;
   * roznica jest taka, ze wtedy chodzilo o `chromePixels`, a dzis o `textElements`.
   *
   * ZMYSLONE JEST TYLKO TYLE, ILE TRZEBA, ZEBY SCENA BYLA WIDOKIEM PRACY: jeden agent bez
   * historii i jeden workflow o jednym kroku. Zaden bieg, zadna notatka, zadna umiejetnosc
   * i zaden skill nie jest udawany — reszta `list_*` dalej oddaje pusta liste. */
  const ONE_AGENT = [
    {
      kind: 'healthy',
      value: {
        schema: 1,
        id: 'density-agent',
        name: 'Builder',
        summary: 'Writes the change',
        color: 'green',
        instructions: 'Make the smallest change that works.',
        runsWith: 'claude',
        model: 'sonnet',
        thinking: 'balanced',
        fileAccess: 'edit-in-folder',
        giveUpAfterMinutes: 45,
        tools: 'all',
        reachesTheWeb: false,
        skills: [],
        connections: [],
        writeResultsTo: '',
      },
    },
  ];
  const ONE_WORKFLOW = [
    {
      kind: 'healthy',
      value: {
        path: 'density-scene.json',
        place: 'library',
        workflow: {
          schema: 1,
          id: 'density-scene',
          name: 'Ship a feature',
          steps: [{ kind: 'agent', id: 's1', name: 'Build', agent: 'density-agent' }],
          links: [],
        },
      },
    },
  ];
  host.__TAURI_INTERNALS__ = {
    transformCallback: (callback) => {
      const id = Math.floor(Math.random() * 1e9);
      host['_' + String(id)] = callback;
      return id;
    },
    unregisterCallback: () => undefined,
    invoke: (command) =>
      Promise.resolve(
        command === 'list_workspaces'
          ? ONE_WORKSPACE
          : command === 'list_agents'
            ? ONE_AGENT
            : command === 'list_workflows'
              ? ONE_WORKFLOW
              : command.startsWith('list_')
                ? []
                : command === 'new_id'
                  ? 'id-0'
                  : null,
      ),
  };
}

/** Siedem metryk z `docs/ARCHITECTURE.md` §7 — cztery jako liczby, trzy jako powód. */
function measure() {
  const seen = (element) => {
    const box = element.getBoundingClientRect();
    if (box.width === 0 || box.height === 0) return false;
    const style = getComputedStyle(element);
    return style.visibility !== 'hidden' && style.display !== 'none' && style.opacity !== '0';
  };

  const LANDMARKS = 'main, nav, header, footer, aside, [role="region"], [role="dialog"]';
  const labelled = [...document.querySelectorAll(LANDMARKS)]
    .filter(seen)
    .concat(
      [...document.querySelectorAll('section[aria-label], section[aria-labelledby], [aria-label]')]
        .filter((element) => element.matches('section, [role]'))
        .filter(seen),
    );
  const labelledRegions = new Set(labelled).size;

  /* CHROME MIERZONY GEOMETRIĄ, nie sumą nazwanych stałych. Asercja `TITLEBAR_HEIGHT <= 96`
     była w tym repo ZIELONA przy 138 px realnego chrome, bo liczyła jeden pasek z trzech
     (`src/ui/shell/chrome-budget.test.ts`, nagłówek). Górnej krawędzi treści nie da się tak
     pomylić — ale TYLKO wtedy, gdy wiadomo, co jest treścią.

     PIERWSZA WERSJA TEGO POMIARU BRAŁA „pierwszy element z tekstem wewnątrz `main`" i dała
     11 px, bo trafiła w przycisk `＋` na pasku kart. To jest dokładnie ta klasa cichego
     zaniżenia, dla której to sprawdzenie istnieje. Treść jest więc wskazana KOTWICĄ, a jej
     brak jest powodem odmowy pomiaru, nie pretekstem do zgadywania. */
  /* Kontrola sceny: jeśli zaproszenie pierwszego startu jednak stoi na ekranie, mierzymy
     nie ten widok, o którym mówi §7 — i mamy o tym powiedzieć, a nie oddać większą liczbę. */
  const inviteIsUp = document.querySelector('[data-add-workspace]') !== null;
  const CONTENT_ANCHOR = '[data-work]';
  const content = document.querySelector(CONTENT_ANCHOR);
  const chromePixels =
    content !== null && seen(content) && !inviteIsUp
      ? Math.max(0, Math.round(content.getBoundingClientRect().top))
      : null;

  const textElements = [...document.body.querySelectorAll('*')].filter(
    (element) =>
      seen(element) &&
      [...element.childNodes].some(
        (node) => node.nodeType === 3 && (node.textContent ?? '').trim() !== '',
      ),
  ).length;

  const animatedRegions = [...document.body.querySelectorAll('*')].filter((element) => {
    if (!seen(element)) return false;
    const style = getComputedStyle(element);
    return style.animationName !== 'none' && style.animationName !== '';
  }).length;

  const metrics = { labelledRegions, textElements, animatedRegions };
  if (chromePixels !== null) metrics.chromePixels = chromePixels;

  const notMeasured = {};
  if (chromePixels === null) {
    notMeasured.chromePixels = inviteIsUp
      ? 'the first-run invitation to pick a folder is still on screen, so this is not the view a person works in. Measuring it would report 137 px against a ceiling written for the working view'
      : 'the default view shows no [data-work] region, so there is no first content to measure chrome against. Rename that anchor and this metric stops being a number rather than quietly becoming a smaller one';
  }
  notMeasured.liveRegionsPerFact =
    'this counts live regions PER FACT, and which fact a region expresses is not written in the DOM. Counting live regions alone would answer a different question and pass a screen that says one thing in six places';
  notMeasured.agentCardLines =
    'the default view seeds no agent card: this collector answers the app with empty lists, so counting lines here would measure a card that does not exist';
  notMeasured.navigationAxes =
    'an axis is a question the screen answers, and whether two axes are perpendicular is a human reading of what those questions are. ARCHITECTURE §7 states the limit as "2, and they must be perpendicular"';

  return { metrics, notMeasured };
}

const out = (() => {
  const at = process.argv.indexOf('--out');
  return at > 0 ? process.argv[at + 1] : join(DIST, 'density-snapshot.json');
})();

if (!existsSync(join(DIST, 'index.html'))) {
  console.error('density-collect: dist/index.html is absent — run `npm run build` first');
  process.exit(2);
}

const { server, port } = await serveDist();
const browser = await chromium.launch();
const widths = [];
let notMeasured = {};
try {
  for (const width of WIDTHS) {
    const page = await browser.newPage({ viewport: { width, height: HEIGHT } });
    await page.addInitScript(stubTauri);
    await page.goto(`http://127.0.0.1:${String(port)}/`, { waitUntil: 'networkidle' });
    await page.locator('main[data-section]').waitFor({ state: 'attached', timeout: 30_000 });
    const taken = await page.evaluate(measure);
    widths.push({ width, metrics: taken.metrics });
    notMeasured = { ...notMeasured, ...taken.notMeasured };
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
}

writeFileSync(out, JSON.stringify({ widths, notMeasured }, null, 2) + '\n');
const counted = Object.keys(widths[0]?.metrics ?? {}).length;
console.log(
  `density-collect: ${String(counted)} metrics measured at ${WIDTHS.map(String).join(' and ')} px, ` +
    `${String(Object.keys(notMeasured).length)} left unmeasured with a reason -> ${out}`,
);
