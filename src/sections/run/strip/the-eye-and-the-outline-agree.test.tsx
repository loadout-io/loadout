/* Stopień nagłówka i jego poziom mówią o ekranie biegu TO SAMO.
 *
 * ZMIERZONA WADA, 2026-08-31. Produkcyjny ekran biegu wydaje cztery nagłówki i dwa z nich
 * niosą sprzeczne odpowiedzi na jedno pytanie „co tu jest najważniejsze":
 *
 *     h1   15 px (`text-heading`)   „Run"             ← `./strip.tsx`
 *     h2   34 px (`text-hero`)      „Ship a feature"  ← `./head.tsx`
 *     h2   11 px (`text-eyebrow`)   „Steps"           ← `../index.tsx`
 *     h1   40 px (`text-display`)   powitanie         ← `../first-run.tsx`
 *
 * Oko czyta 34 px jako rzecz ważniejszą od 15 px. Czytnik ekranu czyta `h1` jako rzecz
 * ważniejszą od `h2`. Na tym ekranie są to DWA RÓŻNE napisy — więc ten sam ekran mówi dwie
 * różne rzeczy zależnie od tego, czym się go czyta, a nazwa biegu jest w spisie nagłówków
 * PODRZĘDNA wobec nazwy sekcji, która stoi nad nią w pasku i mierzy pół jej wysokości.
 *
 * MAKIETA JEST TU WYROCZNIĄ I NIE MA W NIEJ TEJ SPRZECZNOŚCI. `docs/mockup/index.html` pisze
 * nazwę biegu jako `<h1 class="sm">` (34 px), a jej pasek — ten sam, który u nas niesie „Run" —
 * nie ma nagłówka w ogóle: `.strip` to szukajka, przewodnik, pastylka biegu i inicjały.
 *
 * ── DWA PUNKTY, BO WADA MA DWIE POŁOWY ──────────────────────────────────────────────────────
 *
 * PIERWSZY pyta o tę parę wprost: dwa napisy, które nazywają ten ekran, i tylko one. Wersją
 * słabą byłoby `expect(markup).toContain('<h1')` — przechodzi dziś, przy odwróconej parze,
 * bo `<h1>` na ekranie JEST, tylko nosi cichszy napis.
 *
 * DRUGI pyta o cały ekran, ale wyłącznie o to, co jest od nazwy biegu CICHSZE. Nagłówek
 * głośniejszy od niej (dziś: powitanie pierwszego uruchomienia, 40 px) nie wchodzi do tego
 * porównania i to jest decyzja, nie przeoczenie: `../first-run.tsx` nie należy do tego zadania,
 * a kryterium, które sądzi cudzy plik, przewraca się od cudzej naprawy i mówi wtedy o niczym.
 * Granica jest więc wypowiedziana: ten plik NIE odpowiada na pytanie, czy 40 px nad nazwą biegu
 * jest właściwym stopniem dla drzwi pierwszego otwarcia.
 *
 * CZEGO STĄD NIE WIDAĆ, powiedziane wprost. Dwa nagłówki tej samej klasy mieszkają poza tą
 * sceną i ten plik ich nie ogląda: `<h1>` z komendą trzymanego terminala (`../rail/rail.tsx`,
 * 12 px) i `<h1>` z nazwą karty (`../session/session.tsx`, 22 px). Oba są `h1` cichszym od
 * nazwy biegu, oba montują się w stanach, których ta scena nie stawia, i oba leżą poza
 * obszarem tego zadania. Zgłoszone, nie obchodzone.
 *
 * STOPIEŃ CZYTAMY Z DRABINKI, NIE Z NAZWY KLASY. `text-hero` w markupie samo z siebie nie
 * znaczy 34 px — znaczy dopiero razem z `--text-hero` w `src/styles/theme.css`. Ten sam zabieg,
 * z tego samego powodu, stoi w `./the-run-head-is-the-mockup-head.test.tsx`.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby specyfikacja padała na
 * asercji o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import { sectionEntry } from '../../../ui/sections';
import Run from '../index';
import { setBudgetUsd } from '../limits/chosen';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

const theme = fileText(THEME);

const WORKFLOW = 'Ship a feature';
const WORKSPACE = '/Users/someone/Projects/atlas';

const STEPS: readonly Step[] = [
  { id: 'reproduce', name: 'Reproduce', state: 'succeeded', kind: 'agent' },
  { id: 'fix', name: 'Fix', state: 'succeeded', kind: 'agent' },
  { id: 'tests', name: 'Tests pass', state: 'running', kind: 'check' },
  { id: 'second', name: 'Second opinion', state: 'pending', kind: 'agent' },
];

function done(id: number, costUsd: number | null): FeedLine {
  return {
    kind: 'done',
    agent: 'Scout',
    text: 'Done',
    turns: 1,
    durationMs: 62_000,
    costUsd,
    inputTokens: 10,
    outputTokens: 20,
    cachedTokens: 0,
    ended: 'well',
    id,
    at: Date.UTC(2026, 7, 31, 9, 41, 7) + id,
  };
}

/** Produkcyjny ekran Run dla biegu, który idzie — ta sama scena, co w sąsiednim pliku. */
function runningScreen(): string {
  setBudgetUsd(75);
  useRun.setState({
    workflow: WORKFLOW,
    steps: STEPS,
    folder: WORKSPACE,
    agents: ['Scout', 'Builder', 'Needle'],
    lines: [done(1, 3.41)],
    droppedBefore: 0,
  });
  return renderToStaticMarkup(<Run />);
}

/** Jeden nagłówek tak, jak stoi w markupie: poziom, stopień w pikselach i widoczny napis. */
interface Heading {
  /** Cyfra ze znacznika: 1 dla `<h1>`. Mniejsza liczba znaczy WYŻEJ w spisie. */
  readonly level: number;
  /** Stopień drabinki w pikselach, odczytany z `src/styles/theme.css`. */
  readonly pixels: number;
  /** To, co widzi człowiek — bez zagnieżdżonych znaczników. */
  readonly said: string;
  /**
   * Czy ten nagłówek jest tym, który nazywa SEKCJĘ (`data-section-name` z `./strip.tsx`).
   *
   * Znacznikiem, a nie porównaniem napisów: nazwa sekcji przyjeżdża z rejestru i wolno jej
   * zmienić brzmienie, a wtedy szukanie po treści przestałoby cokolwiek znajdować i ten plik
   * zzieleniałby na pustce. Ten sam atrybut jest po to, żeby wyrażenie `[data-strip] h1`
   * w `e2e/tests/the-run-strip-fits-its-window.spec.ts` miało w co celować poza numerem
   * znacznika — powód w całości stoi przy tym atrybucie w `./strip.tsx`.
   */
  readonly namesTheSection: boolean;
  /** Gdzie ten nagłówek zaczyna się w markupie; służy wyłącznie do odróżnienia dwóch takich samych. */
  readonly at: number;
}

/**
 * Ile pikseli niesie stopień drabinki o tej nazwie. `null` znaczy „nie ma takiego stopnia".
 *
 * Szukamy `--text-<nazwa>:` z dwukropkiem TUŻ za nazwą, bo obok każdego stopnia stoją jego
 * `--text-<nazwa>--line-height` i `--text-<nazwa>--font-weight`. Bez dwukropka „hero"
 * dopasowałoby się do interlinii i oddało 1.08 piksela.
 */
function ladderStep(name: string): number | null {
  const found = new RegExp('--text-' + name + '\\s*:\\s*([0-9.]+)px').exec(theme);
  return found === null ? null : Number(found[1]);
}

/** Nazwy klas `text-*` z jednego znacznika otwierającego. */
function ladderNames(tag: string): readonly string[] {
  const declared = /class="([^"]*)"/.exec(tag)?.[1] ?? '';
  return declared
    .split(/\s+/)
    .filter((name) => name.startsWith('text-'))
    .map((name) => name.slice('text-'.length));
}

/**
 * Każdy nagłówek ekranu, w kolejności markupu.
 *
 * Nagłówek, którego stopnia nie da się odczytać z drabinki, NIE jest po cichu pomijany:
 * wchodzi tu z `pixels: -1` i pierwszy punkt niżej odmawia wtedy porównania. Cicho pominięty
 * byłby dokładnie tą wadą, którą ten plik ma łapać — nagłówkiem o nieznanej wadze.
 */
function headingsOf(markup: string): readonly Heading[] {
  return [...markup.matchAll(/<h([1-6])\b([^>]*)>([\s\S]*?)<\/h\1>/g)].map((hit) => {
    const attributes = hit[2] ?? '';
    const steps = ladderNames('<h ' + attributes + '>')
      .map(ladderStep)
      .filter((pixels): pixels is number => pixels !== null);
    const only = steps.length === 1 ? steps[0] : undefined;
    return {
      level: Number(hit[1] ?? '0'),
      pixels: only ?? -1,
      said: (hit[3] ?? '').replace(/<[^>]*>/g, '').trim(),
      namesTheSection: /\bdata-section-name\b/.test(attributes),
      at: hit.index,
    };
  });
}

/** Krótki opis nagłówka do komunikatu porażki. */
function describeHeading(heading: Heading): string {
  return (
    'h' +
    String(heading.level) +
    ' at ' +
    String(heading.pixels) +
    'px saying ' +
    JSON.stringify(heading.said.slice(0, 40))
  );
}

beforeEach(() => {
  setBudgetUsd(null);
  useRun.setState({
    workflow: '',
    steps: [],
    folder: null,
    agents: [],
    lines: [],
    droppedBefore: 0,
  });
});

describe('the run screen weighs its headings the same way the eye does', () => {
  it('names the run above the name of the section, in the outline as well as in the type', () => {
    const markup = runningScreen();
    const headings = headingsOf(markup);

    expect(
      headings.every((heading) => heading.pixels > 0),
      'one of the headings on this screen declares no step of the ladder from ' +
        'src/styles/theme.css, or declares two of them at once, so how loud it is cannot be ' +
        'read at all and every comparison below would be running past it. Read: ' +
        headings.map(describeHeading).join(', '),
    ).toBe(true);

    const label = sectionEntry('run').label;
    const marked = headings.filter((heading) => heading.namesTheSection);
    const run = headings.find((heading) => heading.said.startsWith(WORKFLOW));

    expect(
      marked.map(describeHeading),
      'exactly one heading on this screen says which section a person is standing on, and ' +
        'that is not what came out. Zero of them means the comparison below would pass on ' +
        'nothing at all; two of them means the screen answers the same question twice, and ' +
        'this point would then measure whichever answer happens to be written first.',
    ).toHaveLength(1);
    const section = marked[0];
    if (section === undefined || marked.length !== 1) return;

    expect(
      run === undefined,
      'this screen names no run, so the comparison below would pass on nothing. It was asked ' +
        'for a run called ' +
        JSON.stringify(WORKFLOW) +
        '. Read: ' +
        headings.map(describeHeading).join(', '),
    ).toBe(false);
    if (run === undefined) return;

    expect(
      section.said,
      'the heading that says which section a person is standing on says something else than ' +
        'the registry in src/ui/sections.tsx does. Either that mark moved onto another ' +
        'heading, and then every comparison below is about the wrong one, or the name on the ' +
        'screen and the name in the side menu have grown apart.',
    ).toBe(label);

    expect(
      run.pixels > section.pixels,
      'the name of the run is no longer written louder than the name of the section (' +
        describeHeading(run) +
        ' against ' +
        describeHeading(section) +
        '), so the two halves of this point have stopped disagreeing by growing quiet ' +
        'instead of by lining up. The mockup writes the run as the loudest thing on this ' +
        'screen and this file only asks that the outline say the same.',
    ).toBe(true);

    expect(
      run.level,
      'the eye and the outline disagree about what this screen is. The name of the run is ' +
        describeHeading(run) +
        ' and the name of the section is ' +
        describeHeading(section) +
        ': a person reading with their eyes is told the run matters most, and a person ' +
        'reading with a screen reader is told the section does, because it is the higher ' +
        'heading. One screen, two answers, and the louder one is the one nobody hears.',
    ).toBeLessThan(section.level);
  });

  it('lets nothing quieter than the run outrank it in the outline', () => {
    const markup = runningScreen();
    const headings = headingsOf(markup);
    const run = headings.find((heading) => heading.said.startsWith(WORKFLOW));

    expect(
      run,
      'the run screen names no run at all, so there is no heading to measure the quieter ones ' +
        'against and this point would pass on an empty comparison. Read: ' +
        headings.map(describeHeading).join(', '),
    ).toBeDefined();
    if (run === undefined) return;

    const quieter = headings.filter(
      (heading) => heading.pixels > 0 && heading.pixels < run.pixels && heading.at !== run.at,
    );

    expect(
      quieter.length,
      'not one heading on this screen is written quieter than the name of the run, so the ' +
        'comparison below has nothing to compare and would pass on an empty list. Read: ' +
        headings.map(describeHeading).join(', '),
    ).toBeGreaterThan(0);

    const outranking = quieter.filter((heading) => heading.level <= run.level);

    expect(
      outranking.map(describeHeading),
      'these headings are written quieter than the name of the run (' +
        describeHeading(run) +
        ') and still stand at or above it in the outline. Every one of them tells a person ' +
        'reading by heading that it is at least as important as the run they are watching, ' +
        'while the screen in front of them says the opposite in half the height.',
    ).toEqual([]);
  });
});
