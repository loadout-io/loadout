/* Nad pracą stoi JEDEN pas, a nazwa biegu jest w nim rozpoznawalna, nie dominująca.
 *
 * ZGŁOSZENIE WŁAŚCICIELA, 2026-09-01, słowo w słowo: „tu sie za duzo dzieje i na pewno za duzy
 * jest ten napis workflow i ogolnie trzeba odchudzic ten widok, stanowo za duzy haos".
 *
 * ── CO ZMIERZONO, ZANIM POWSTAŁ TEN PLIK (chromium, 1512×950, zbudowane `dist/`) ──────────────
 *
 * Nad pierwszym krokiem stały CZTERY pasy jeden pod drugim, razem 241 px:
 *
 *     pasek loadoutu        52 px   (chrome — liczy go `scripts/density-collect.mjs`)
 *     nagłówek biegu       104 px   (treść: nadoczko, tytuł 34 px, metadana)
 *     wiersz wyboru         51 px   (treść: lista workflow + zdanie „kto to wybrał")
 *     nagłówki kolumn       34 px   (treść: „STEPS")
 *
 * Dwa środkowe pasy niosą JEDNĄ rzecz — który bieg to jest i który ruszy — i każdy z nich miał
 * własną kreskę pod sobą. To są dwa pasy za jedną odpowiedź.
 *
 * ── DWA PUNKTY, BO ZGŁOSZENIE MA DWIE POŁOWY ─────────────────────────────────────────────────
 *
 * PIERWSZY liczy PASY, nie piksele: to repo nie ma jsdom, a wysokość pasa jest funkcją arkusza,
 * której bez okna nie da się rozstrzygnąć. Kreska pod pasem jest za to w markupie i jest tym,
 * co oko liczy jako „kolejne piętro". Wersją słabą byłoby `expect(markup).toContain('border-b')`
 * — przechodzi ona dziś, przy dwóch pasach, bo kreska JEST.
 *
 * DRUGI pyta o STOPIEŃ tytułu i czyta oczekiwaną wartość z makiety W TYM SAMYM biegu. Wersją
 * słabą jest `expect(markup).toContain('text-title')` — przechodzi ona, gdy ten sam napis stoi
 * gdziekolwiek indziej w markupie (a stoi: `../past/panel.tsx`, `../session/session.tsx`),
 * i przechodzi, gdy drabinka zmieni wartość pod tą nazwą. Dlatego stopień czytany jest z KLASY
 * stojącej na elemencie z `data-run-title`, przez drabinkę z `src/styles/theme.css`, i dopiero
 * ta liczba jest porównywana z rysunkiem.
 *
 * GRANICA, POWIEDZIANA WPROST, bo sprawdzenie z nieopisaną granicą jest gorsze niż jego brak:
 * ten plik NIE odpowiada na pytanie, ile pikseli wysokości ma pas po zmianie. Odpowiada na to
 * pomiar w chromium, ręczny, i jego liczby stoją w raporcie do właściciela. Tutaj sądzone jest
 * to, co da się osądzić bez okna: ile pasów, i w jakim stopniu napisany jest tytuł.
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby specyfikacja padała na
 * asercji o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { useRun } from '../../../state/run';
import { useWorkspaces } from '../../../state/workspaces';
import type { Choice } from '../choices';
import Run from '../index';
import { setBudgetUsd } from '../limits/chosen';
import { forgetWhatIsReady, rememberAgents, rememberRuns, rememberWorkflows } from '../whats-ready';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

const html = fileText(MOCKUP);
const theme = fileText(THEME);

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  return new RegExp(selector.replace(/\./g, '\\.') + '\\s*\\{([^}]*)\\}').exec(css)?.[1] ?? '';
}

/** Wartość jednej właściwości z ciała reguły, bez odstępów. */
function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;|\\n)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return (found?.[1] ?? '').trim();
}

/** Ile pikseli niesie stopień drabinki o tej nazwie. `null` znaczy „nie ma takiego stopnia". */
function ladderStep(name: string): number | null {
  const found = new RegExp('--text-' + name + '\\s*:\\s*([0-9.]+)px').exec(theme);
  return found === null ? null : Number(found[1]);
}

/** Znacznik otwierający elementu niosącego ten atrybut, razem z całym jego stylem. */
function openingTag(markup: string, attribute: string): string {
  return new RegExp('<[a-z0-9]+[^>]*\\b' + attribute + '="[^"]*"[^>]*>').exec(markup)?.[0] ?? '';
}

/** Stopień drabinki, w pikselach, zadeklarowany klasą na tym znaczniku. `null`, gdy nie jeden. */
function stepOfTag(tag: string): number | null {
  const declared = /class="([^"]*)"/.exec(tag)?.[1] ?? '';
  const steps = declared
    .split(/\s+/)
    .filter((name) => name.startsWith('text-'))
    .map((name) => ladderStep(name.slice('text-'.length)))
    .filter((pixels): pixels is number => pixels !== null);
  return steps.length === 1 ? (steps[0] ?? null) : null;
}

const HERE = { id: '/Users/x/ledger-ui', name: 'Ledger', folder: '/Users/x/ledger-ui' };

const MURMUR: Choice = {
  path: 'murmur-1.json',
  name: 'Murmur-1',
  steps: [
    { id: 'gather', name: 'Gather', state: 'pending', kind: 'agent', at: { x: 40, y: 40 } },
    { id: 'write', name: 'Write it up', state: 'pending', kind: 'agent', at: { x: 40, y: 170 } },
  ],
  links: [],
};

/** Produkcyjny ekran w stanie ze zrzutu: setup gotowy, nic nie biegnie, jest co uruchomić. */
function readyScreen(): string {
  useWorkspaces.setState({ all: [HERE], activeId: HERE.id, said: null });
  useRun.setState({ workflow: '', steps: [], links: null });
  rememberWorkflows([MURMUR]);
  rememberAgents(1);
  rememberRuns(HERE.folder, []);
  return renderToStaticMarkup(<Run />);
}

beforeEach(() => {
  setBudgetUsd(null);
  forgetWhatIsReady();
  useRun.setState({
    workflow: '',
    steps: [],
    folder: null,
    agents: [],
    lines: [],
    droppedBefore: 0,
  });
});

describe('one band stands over the work, and the run is named inside it', () => {
  it('keeps the workflow choice inside the head of the run instead of under it', () => {
    const markup = readyScreen();
    const head = markup.indexOf('data-run-head');
    const choice = markup.indexOf('data-workflow-choice');
    const plan = markup.indexOf('data-plan-column');

    expect(
      Math.min(head, choice, plan),
      'one of the three regions this point compares is not on this screen at all, so the ' +
        'ordering below would be comparing against a -1 and passing on nothing. It was asked ' +
        'for a screen with one runnable workflow and no run going.',
    ).toBeGreaterThanOrEqual(0);

    const closes = markup.indexOf('</header>', head);
    expect(
      closes,
      'the head of the run never closes, so there is no inside for the choice to be in and ' +
        'the point below would measure an unbounded slice of the screen.',
    ).toBeGreaterThan(head);
    expect(
      choice > head && choice < closes,
      'the control that picks which workflow runs stands OUTSIDE the head of the run, in a ' +
        'band of its own. Measured in chromium at 1512x950: that band costs 51 px and a rule ' +
        'of its own, directly under a head that already costs 104 px and announces the very ' +
        'same workflow. Two floors for one answer is the whole of the report — the screen has ' +
        'one hero and on this screen it is the work, not the two bands of chrome over it.',
    ).toBe(true);

    const above = markup.slice(markup.indexOf('data-work='), plan);
    const rules = [...above.matchAll(/border-b\b/g)].length;
    expect(
      rules,
      'the screen stacks ' +
        String(rules) +
        ' horizontal rules between the top of the work area and the first of its two columns, ' +
        'and every one of them reads to the eye as another floor to get past before the work ' +
        'starts. One band names the run and offers to change it; a second one under it says ' +
        'the same thing twice.',
    ).toBe(1);
  });

  it('writes the run title in the step the drawing gives a panel title, not a screen title', () => {
    const panel = property(ruleBody(html, 'h2'), 'font-size');
    expect(
      panel,
      'nothing was read out of the `h2` rule in docs/mockup/index.html, so this point has no ' +
        'step to compare the title against and both comparisons below would pass on empty ' +
        'strings. That rule is the step the drawing gives the title of a card, a panel or a ' +
        'dialog.',
    ).not.toBe('');

    const markup = readyScreen();
    const tag = openingTag(markup, 'data-run-title');
    expect(
      tag,
      'the head of the run marks no element as the title of the run, so the step it is written ' +
        'in cannot be read at all and everything below would be measuring some other heading.',
    ).not.toBe('');

    const written = stepOfTag(tag);
    expect(
      written,
      'the title of the run declares no step of the ladder from src/styles/theme.css, or ' +
        'declares two of them at once, so how loud it is cannot be read. Read: ' +
        JSON.stringify(tag),
    ).not.toBeNull();

    expect(
      String(written) + 'px',
      'the name of the run is written louder than the drawing writes the title of a panel (' +
        panel +
        '), and on this screen that is the wrong size. The title of the first-run door is the ' +
        'hero of ITS screen because there is nothing else on it; here the hero is the work — ' +
        'the steps and the stream — and the name of the run only has to be recognisable over ' +
        'them. Written at ' +
        String(written) +
        'px it is the loudest thing a person sees before any of the work.',
    ).toBe(panel);

    const drawn = property(ruleBody(html, '.rhead h1'), 'font-size');
    expect(
      drawn,
      'the `.rhead h1` rule of docs/mockup/index.html says nothing about the size of the run ' +
        'title, so the drawing and the screen no longer answer the same question and the ' +
        'screen above is the only place this decision is written down. The mockup is the ' +
        'oracle for this head: change one, change the other.',
    ).not.toBe('');
    expect(
      drawn,
      'the drawing and the screen disagree about how loud the run title is. The drawing writes ' +
        'it at ' +
        drawn +
        ' and the screen at ' +
        String(written) +
        'px.',
    ).toBe(panel);
  });
});
