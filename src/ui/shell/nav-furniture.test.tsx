/* AC-3 dla T-37: marka i status są na swoim miejscu, a światła macOS niczego nie zasłaniają.
 *
 * SŁABA WERSJA: sprawdzenie samej obecności napisu `LOADOUT`. Przechodzi ono wtedy, gdy marka
 * leży POD światłami i jest nieczytelna — a to jest jedyny powód, dla którego ten punkt istnieje.
 * Okno stoi na `titleBarStyle: "Overlay"` i `hiddenTitle`, więc trzy światła pływają NAD treścią
 * w lewym górnym rogu, czyli dokładnie tam, gdzie makieta stawia markę. Makieta jest stroną WWW
 * i tego nie modeluje; adaptacja należy do tego zadania i musi być mierzona, nie zadeklarowana.
 *
 * Dlatego punkt o świetle wiąże DWIE rzeczy naraz: odstęp przeczytany z wyrenderowanej
 * nawigacji i `trafficLightPosition.y` przeczytane z `src-tauri/tauri.conf.json`. Osobno każda
 * z nich wygląda rozsądnie przy dowolnej wartości drugiej.
 *
 * ROZBIEŻNOŚĆ Z BRZMIENIEM KRYTERIUM, zgłoszona zamiast obejścia (AGENTS.md §7). Kryterium każe
 * czytać z `tauri.conf.json` także **wysokość świateł**. Tej liczby w tym pliku NIE MA — jest
 * tam wyłącznie `trafficLightPosition` z `x` i `y`. Wysokość świateł jest stałą fizyczną macOS,
 * nie naszą konfiguracją, więc stoi niżej jako nazwana stała z tym samym uzasadnieniem i tą samą
 * wartością, co `LIGHTS_PLUS_GAP` w `window.test.tsx` (niezmiennik 13: jedna liczba, jedno
 * znaczenie). Z konfiguracji czytamy to, co w niej faktycznie jest.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 *
 * ZMIANA PUNKTU O STOPCE, 2026-09 (Z-34). Do 2026-08-18 brzmiał on „stopka nazywa KAŻDEGO
 * dostawcę z unii `Vendor`", potem — „nazywa każdego, kogo aplikacja umie uruchomić, i ani
 * jednego, którego nie umie", czytając mapowanie sterowników z `src-tauri/src/lib.rs`. Obie
 * wersje sądziły NAPIS: pytały, czy zaszyte w kod zdanie zgadza się z zaszytą w kod tabelą typów.
 * Ani jedna z nich nie umiała zobaczyć tego, co człowiek widzi naprawdę — bo o to nikt nie pytał
 * ani binarki, ani granicy.
 *
 * Dziś stopka rysuje ODCZYT (`check_agent_apps` → `state/agent-apps.ts`), więc ten punkt karmi
 * magazyn prawdziwą odpowiedzią granicy i pyta o zdania, które z niej powstały — w obu trybach
 * bocznego menu. Zwinięta kolumna ma je oddać co do słowa w podpowiedzi: rzecz znika stąd tylko
 * wtedy, kiedy jej brak nie kłamie.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../App';
import { useAgentApps } from '../../state/agent-apps';
import { collapseNav } from '../../state/settings';
import { PANE_GAP, SideNav } from './titlebar';

/* Atrapa granicy, podniesiona razem z `vi.mock`. Stopka i tryb menu jadą przez prawdziwe
 * `invoke`, więc bez niej ten plik pytałby żywego Tauri; z nią mierzy dokładnie to, co okno
 * zrobiło z odpowiedzią, którą scena podstawiła. */
const { invoked, answer } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined as unknown)),
  answer: { of: undefined as unknown },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...sent: unknown[]) => {
    invoked(...sent);
    return Promise.resolve(sent[0] === 'check_agent_apps' ? answer.of : undefined);
  },
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const CONF = resolve(ROOT, 'src-tauri/tauri.conf.json');

/** Co powiedziała granica: jedna aplikacja z wersją, druga nieobecna. Dwa różne zdania. */
const ANSWERED = [
  { app: 'claude-code', state: 'found', version: 'claude-code 9.9.9' },
  { app: 'codex', state: 'not-found' },
];

const FOUND_CLAUDE = 'Claude Code · claude-code 9.9.9';
const NO_CODEX = "Codex wasn't found.";

/** Napis, który stał w tym miejscu, dopóki nikt niczego nie pytał. */
const RETIRED = 'Claude · Codex ready';

/**
 * Wysokość trzech świateł plus odstęp, licząc od `trafficLightPosition.y`. Stała fizyczna
 * macOS, nieobecna w `tauri.conf.json` — patrz nagłówek. Ta sama wartość co `LIGHTS_PLUS_GAP`
 * w `window.test.tsx`: 20 px świateł + 8 px odstępu.
 */
const LIGHTS_PLUS_GAP = 28;

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** `trafficLightPosition.y` z konfiguracji okna, albo `null`, gdy pliku albo pola nie ma. */
function lightsTop(): number | null {
  const raw = fileText(CONF);
  if (raw.trim() === '') return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
  const windows = (parsed as { app?: { windows?: unknown } }).app?.windows;
  if (!Array.isArray(windows) || windows.length === 0) return null;
  const y = (windows[0] as { trafficLightPosition?: { y?: unknown } }).trafficLightPosition?.y;
  return typeof y === 'number' ? y : null;
}

/** Zadeklarowany górny odstęp wyrenderowanej nawigacji. */
function navPaddingTop(markup: string): number | null {
  const nav = /<nav[^>]*>/.exec(markup)?.[0] ?? '';
  const found = /padding-top:\s*(\d+)px/.exec(nav);
  return found === null ? null : Number(found[1]);
}

/** Treść stopki: ostatni blok nawigacji, po ostatnim przełączniku sekcji. */
function footerHtml(markup: string): string {
  const lastSwitch = markup.lastIndexOf('data-section-switch');
  if (lastSwitch < 0) return '';
  const afterButton = markup.indexOf('</button>', lastSwitch);
  return afterButton < 0 ? '' : markup.slice(afterButton + '</button>'.length);
}

/**
 * Znacznik otwierający nośnika stopki — tego samego w obu trybach.
 *
 * Kotwica jest atrybutem, a nie kształtem: w trybie rozwiniętym to blok ze zdaniami, w zwiniętym
 * sama kropka, więc szukanie po klasie albo po nazwie elementu sądziłoby układ zamiast treści.
 */
function statusTag(markup: string): string {
  return /<[a-z]+[^>]*data-agent-apps-status[^>]*>/.exec(markup)?.[0] ?? '';
}

/** Wartość atrybutu ze znacznika otwierającego, albo pusty napis. */
function attribute(tag: string, name: string): string {
  return new RegExp('\\b' + name + '="([^"]*)"').exec(tag)?.[1] ?? '';
}

/**
 * Markup → to, co człowiek naprawdę czyta.
 *
 * Encje wracają do znaków, bo `renderToStaticMarkup` zamienia apostrof na `&#x27;` i zdanie
 * „Codex wasn't found." przestałoby pasować do samego siebie.
 */
function readable(html: string): string {
  return html
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, '&');
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Boczne menu po prawdziwym odczycie granicy, w tym trybie. */
async function navAfterRead(collapsed: boolean): Promise<string> {
  await collapseNav(collapsed);
  await useAgentApps.getState().check();
  return renderToStaticMarkup(<SideNav section="run" />);
}

const navMarkup = renderToStaticMarkup(<SideNav section="run" />);
const appMarkup = renderToStaticMarkup(<App section="run" screens={{}} />);

beforeEach(() => {
  invoked.mockClear();
  answer.of = ANSWERED;
});

afterEach(async () => {
  await collapseNav(false);
});

describe('the nav carries the brand and the status, and the lights cover neither', () => {
  it('carries the mark and the word LOADOUT, with the mark first', () => {
    /* PRZEPISANE 2026-08-19 (T-49). Do tej pory ten punkt zadal napisu `LOADOUT` i znaku
     * `rotate-45`. Oba przestaly istniec: logotyp jest MALYMI LITERAMI w kroju jezyka ludzkiego
     * (mono w tym systemie znaczy „to wyprodukowala maszyna"), a znak jest najmniejszym
     * PRAWDZIWYM grafem — cztery luzne kwadraty nie mialy krawedzi, wiec nie byly grafem.
     * Punkt sadzi to samo pytanie: czy nawigacja niesie marke i czy znak stoi PRZED slowem. */
    expect(navMarkup, 'the nav does not carry the product name at all').toContain('loadout');
    expect(
      /LOADOUT/.test(navMarkup),
      'the logotype shouts in capitals again. Capitals with wide tracking in a monospace face are ' +
        'a quotation from a terminal, not a logotype: mono in this system means "a machine ' +
        'produced this", and the product name is human language.',
    ).toBe(false);

    const markAt = navMarkup.indexOf('<svg');
    const wordAt = navMarkup.indexOf('loadout');
    expect(
      markAt,
      'the mark is gone. It is four squares turned 45°, straight from `.mark` in the mockup, ' +
        'and it is the only piece of identity this application has.',
    ).toBeGreaterThanOrEqual(0);
    expect(
      markAt,
      'the mark has to come before the word, as it does in the mockup — otherwise the top of ' +
        'the nav reads as a label with a decoration after it.',
    ).toBeLessThan(wordAt);
  });

  it('pins a footer to the bottom with a standing dot, saying what really answered', async () => {
    const wide = footerHtml(await navAfterRead(false));

    expect(
      wide.trim(),
      'there is nothing after the last section switch, so the nav has no footer. The footer is ' +
        'the only place where this application says anything about its surroundings.',
    ).not.toBe('');
    expect(
      wide,
      'the footer is not pinned to the bottom. The mockup `.foot` rule does it with ' +
        '`margin-top:auto`; without it the status floats right under the switches and the nav ' +
        'stops having a bottom edge to read.',
    ).toContain('mt-auto');
    expect(
      wide,
      'the footer carries no status dot. DESIGN §5 says circles exist for exactly one thing — ' +
        'status dots — so this is the one place a `rounded-full` belongs.',
    ).toContain('rounded-full');

    /* KONTROLA PRZECIW PUSTEMU PORÓWNANIU. Gdyby stopka nigdy nie zapytała granicy, oba zdania
     * niżej mogłyby stać w kodzie jako napisy i ten punkt nie odróżniłby ich od odczytu. */
    expect(
      invoked.mock.calls.map((call) => call[0]),
      'nothing in the shell asked the boundary what the two local apps say, so the sentences ' +
        'below could be typed into the component and this point would not know the difference.',
    ).toContain('check_agent_apps');

    const said = readable(wide);
    expect(
      said,
      'the footer does not carry what Claude Code answered, although the boundary said it is ' +
        'there and gave its version. Footer says: ' +
        said,
    ).toContain(FOUND_CLAUDE);
    expect(
      said,
      'the footer does not say that Codex was not found, although that is what the boundary ' +
        'answered. A capability the person does NOT have is the half this sentence used to ' +
        'lie about. Footer says: ' +
        said,
    ).toContain(NO_CODEX);
    expect(
      said,
      'the footer is back to promising readiness for everyone, without asking anybody. A ' +
        'control that lies is worse than a missing one (invariant 16).',
    ).not.toContain(RETIRED);
    expect(
      /\bready\b/i.test(said),
      'the footer calls something ready. Being installed is not being signed in, and being ' +
        'signed in is only known once an agent really runs — so nothing here may say "ready". ' +
        'Footer says: ' +
        said,
    ).toBe(false);
  });

  it('folds the sentences into the tooltip of one dot, losing none of them', async () => {
    const narrow = footerHtml(await navAfterRead(true));
    const tag = statusTag(narrow);

    expect(
      tag,
      'the narrowed footer draws no status carrier at all, so folding the nav threw away the ' +
        'only thing this application says about its surroundings.',
    ).not.toBe('');
    expect(
      occurrences(narrow, 'rounded-full'),
      'the narrowed footer leaves more than one dot, or none. Three states of two apps belong ' +
        'on one dot: one fact, one place (invariant 13).',
    ).toBe(1);
    expect(
      tag,
      'the folded dot is not the muted one. The accent means "this is interactive" and the live ' +
        'colour means "this is happening now"; whether a local app answers is neither (DESIGN §3).',
    ).toContain('bg-muted');

    expect(
      readable(narrow),
      'the narrowed footer still writes the sentences out. A list of app names cut to fit a ' +
        '64 px column promises readiness for whoever is no longer on it.',
    ).not.toContain(FOUND_CLAUDE);

    const tip = readable(attribute(tag, 'title'));
    expect(
      tip,
      'the folded dot says nothing about Claude Code. The rule for this column is that a thing ' +
        'disappears only when its absence does not lie, and a silent dot promises nothing and ' +
        'answers nothing. Tooltip says: ' +
        tip,
    ).toContain(FOUND_CLAUDE);
    expect(tip, 'the folded dot says nothing about Codex. Tooltip says: ' + tip).toContain(
      NO_CODEX,
    );
    expect(
      readable(attribute(tag, 'aria-label')),
      'the folded dot has a tooltip and no accessible name, so a screen reader gets a dot and ' +
        'nothing else. When labels leave the screen, that name is the only way to the words.',
    ).toContain(NO_CODEX);
  });

  it('starts below the macOS lights, using the inset the window config implies', () => {
    const y = lightsTop();
    const inset = navPaddingTop(navMarkup);

    expect(
      y,
      'trafficLightPosition.y could not be read out of src-tauri/tauri.conf.json, so the ' +
        'comparison below would have nothing to compare against and would pass on nothing.',
    ).not.toBeNull();
    expect(inset, 'the rendered nav declares no padding-top at all').not.toBeNull();

    /* DWA UKLADY WSPOLRZEDNYCH, poprawione w T-46. Odstep odczytany z markupu jest LOKALNY
     * dla kartki nawigacji, a wymog swiatel jest GLOBALNY dla okna. Dopoki kartka zaczynala sie
     * w punkcie (0,0) okna, te dwie liczby byly tym samym. Odkad PLYWA o `PANE_GAP` nizej,
     * roznia sie dokladnie o ten odstep — i to on jest brakujacym skladnikiem, nie zmiana
     * wymogu: 8 + 36 = 44 spelnia to samo, co dawniej spelnialo samo 44. */
    expect(
      PANE_GAP + (inset ?? 0),
      'the brand sits under the three macOS lights and is unreadable. The window runs with ' +
        'titleBarStyle "Overlay" and hiddenTitle, so the lights float over the content at ' +
        'trafficLightPosition (y=' +
        String(y) +
        '); measured in WINDOW coordinates the pane floats ' +
        String(PANE_GAP) +
        ' px down and then insets its own content, and the total has to be at least ' +
        String(LIGHTS_PLUS_GAP) +
        ' px below the lights. Changing trafficLightPosition without ' +
        'changing the inset is exactly the case this binds: apart, each number looks sensible.',
    ).toBeGreaterThanOrEqual((y ?? 0) + LIGHTS_PLUS_GAP);
  });

  it('declares exactly one drag region in the whole shell', () => {
    expect(
      occurrences(appMarkup, 'data-tauri-drag-region'),
      'a window with more than one drag region drags from places the user reads as content, ' +
        'and a window with none cannot be moved at all — the title bar is hidden. One region, ' +
        'on the nav, is the whole contract.',
    ).toBe(1);
  });
});
