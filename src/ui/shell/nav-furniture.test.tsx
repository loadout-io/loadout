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
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { SideNav } from './titlebar';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const CONF = resolve(ROOT, 'src-tauri/tauri.conf.json');
const AGENTS = resolve(ROOT, 'src/state/agents.ts');

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

/** Nazwy dostawców z unii `Vendor`, skrócone do pierwszego członu (`claude-code` → `claude`). */
function vendorWords(): readonly string[] {
  const union = /export type Vendor =([^;]*);/.exec(fileText(AGENTS))?.[1] ?? '';
  return [...union.matchAll(/'([^']+)'/g)].map((hit) => (hit[1] ?? '').split('-')[0] ?? '');
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

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

const navMarkup = renderToStaticMarkup(<SideNav section="run" />);
const appMarkup = renderToStaticMarkup(<App section="run" screens={{}} />);

describe('the nav carries the brand and the status, and the lights cover neither', () => {
  it('carries the mark and the word LOADOUT, with the mark first', () => {
    expect(navMarkup, 'the nav does not carry the product name at all').toContain('LOADOUT');

    const markAt = navMarkup.indexOf('rotate-45');
    const wordAt = navMarkup.indexOf('LOADOUT');
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

  it('pins a footer to the bottom with a liveness dot and every vendor named', () => {
    const footer = footerHtml(navMarkup);

    expect(
      footer.trim(),
      'there is nothing after the last section switch, so the nav has no footer. The footer is ' +
        'the only place where this application says anything about its surroundings.',
    ).not.toBe('');
    expect(
      footer,
      'the footer is not pinned to the bottom. The mockup `.foot` rule does it with ' +
        '`margin-top:auto`; without it the status floats right under the switches and the nav ' +
        'stops having a bottom edge to read.',
    ).toContain('mt-auto');
    expect(
      footer,
      'the footer carries no liveness dot. DESIGN §5 says circles exist for exactly one thing — ' +
        'status dots — so this is the one place a `rounded-full` belongs.',
    ).toContain('rounded-full');

    const vendors = vendorWords();
    expect(
      vendors.length,
      'no vendors were read out of the Vendor union in src/state/agents.ts, so the check below ' +
        'would pass on an empty list. Decision D3 puts two vendors in v1 and the union is where ' +
        'that fact lives.',
    ).toBeGreaterThan(0);

    const missing = vendors.filter((word) => !footer.toLowerCase().includes(word.toLowerCase()));
    expect(
      missing,
      'the footer does not name every vendor the application can actually run. D3 makes both ' +
        'vendors v1 scope, so a vendor that exists in the Vendor union and not in the footer is ' +
        'a capability the user is never told about. Footer says: ' +
        footer.replace(/<[^>]*>/g, ' '),
    ).toEqual([]);
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

    expect(
      inset ?? 0,
      'the brand sits under the three macOS lights and is unreadable. The window runs with ' +
        'titleBarStyle "Overlay" and hiddenTitle, so the lights float over the content at ' +
        'trafficLightPosition (y=' +
        String(y) +
        '); the nav has to start at least ' +
        String(LIGHTS_PLUS_GAP) +
        ' px below that. Changing trafficLightPosition without ' +
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
