/* Nagłówek ekranu biegu jest nagłówkiem z makiety — i to MAKIETA jest tu wyrocznią.
 *
 * DLACZEGO WARTOŚCI OCZEKIWANE SĄ CZYTANE, A NIE WPISANE. Słabą wersją każdego punktu niżej jest
 * `expect(markup).toContain('186')`. Przechodzi ona w dwóch przypadkach, w których nagłówek jest
 * zepsuty: gdy ta liczba stoi gdziekolwiek w markupie — także jako wysokość czegoś zupełnie
 * innego — i gdy makieta zmieni się na 200, a ekran nie. Odróżnia je to, że oczekiwana wartość
 * jest czytana z `docs/mockup/index.html` W TYM SAMYM biegu tej specyfikacji. Ten sam zabieg
 * stoi w `../run-matches-mockup.test.tsx` na regule `.work`.
 *
 * KONTROLA PRZECIW PUSTEMU PORÓWNANIU. Parser, który cicho nic nie dopasował, daje dwa puste
 * napisy i porównanie przechodzi na niczym. Każdy odczyt z makiety ma więc osobną asercję na to,
 * że coś realnie znalazł.
 *
 * SŁABĄ WERSJĄ CAŁEGO PLIKU jest wołanie `headlineFor()` i sprawdzanie zwróconej wartości.
 * Przechodzi ona nad dokładnie tą wadą, którą to zadanie zamyka: model umie złożyć nagłówek,
 * a do człowieka on nie dociera, bo nikt go nie zamontował. Dlatego renderuje się tu CAŁY
 * produkcyjny ekran Run i czyta jego markup — poza dwoma ostatnimi punktami, które pytają
 * o samą regułę liczenia i mówią to o sobie.
 *
 * STOPIEŃ TYTUŁU MIERZY SIĘ PRZEZ DWA ODCZYTY, nie przez klasę. `text-hero` w markupie nie
 * dowodzi niczego samo z siebie — dowodzi dopiero razem z tym, że `--text-hero` w
 * `src/styles/theme.css` niesie tę samą liczbę, co `h1.sm` w makiecie. Rozjazd którejkolwiek
 * z tych trzech rzeczy jest tu czerwony.
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
import Run from '../index';
import { setBudgetUsd } from '../limits/chosen';
import { headlineFor } from './headline';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..', '..');
const MOCKUP = resolve(ROOT, 'docs/mockup/index.html');
const THEME = resolve(ROOT, 'src/styles/theme.css');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Ciało reguły CSS o podanym selektorze, z pierwszego wystąpienia. */
function ruleBody(css: string, selector: string): string {
  return new RegExp(selector.replace('.', '\\.') + '\\s*\\{([^}]*)\\}').exec(css)?.[1] ?? '';
}

/** Wartość jednej właściwości z ciała reguły, bez odstępów. */
function property(body: string, name: string): string {
  const found = new RegExp('(?:^|;|\\n)\\s*' + name + '\\s*:([^;]*)').exec(body);
  return (found?.[1] ?? '').trim();
}

const html = fileText(MOCKUP);
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

/** Produkcyjny ekran Run dla biegu, który idzie i ma za sobą jedną płatną turę. */
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

/** Znacznik otwierający elementu niosącego ten atrybut, razem z całym jego stylem. */
function openingTag(markup: string, attribute: string): string {
  return new RegExp('<[a-z0-9]+[^>]*\\b' + attribute + '="[^"]*"[^>]*>').exec(markup)?.[0] ?? '';
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

describe('the run screen heads itself with the run, the way the mockup heads it', () => {
  it('draws the three parts of the mockup head, in the order the mockup draws them', () => {
    const head = html.slice(Math.max(html.indexOf('class="rhead"'), 0));
    const eyebrowAt = head.indexOf('class="eyebrow"');
    const titleAt = head.indexOf('<h1');
    const metaAt = head.indexOf('class="meta"');

    expect(
      Math.min(eyebrowAt, titleAt, metaAt),
      'the `.rhead` block of docs/mockup/index.html no longer carries an eyebrow, a title and ' +
        'a line of metadata, so every comparison below would be running against nothing. ' +
        'Either that block moved or it stopped being the head this file reads.',
    ).toBeGreaterThan(0);
    expect(
      eyebrowAt < titleAt && titleAt < metaAt,
      'the mockup itself no longer puts those three in the order this file expects, so the ' +
        'order asserted on the screen below would be somebody\u2019s memory rather than the ' +
        'drawing. Read: eyebrow at ' +
        String(eyebrowAt) +
        ', title at ' +
        String(titleAt) +
        ', metadata at ' +
        String(metaAt),
    ).toBe(true);

    const markup = runningScreen();
    const state = markup.indexOf('data-run-state');
    /* `data-run-title`, nie nazwa stopnia: stopień wolno zmienić (i zmienił się 2026-09-01
       z `text-hero` na `text-title`), a pytanie tego punktu jest o KOLEJNOŚĆ, nie o głośność.
       Nazwa stopnia szukana w całym markupie i tak odpowiadała na inne pytanie: `text-title`
       stoi też w panelu historii i na karcie agenta, więc trafiłaby w cudzy nagłówek. */
    const title = markup.indexOf('data-run-title');
    const meta = markup.indexOf(WORKSPACE.split('/').at(-1) ?? '');

    expect(
      state,
      'the run screen draws no state line over the run at all. Without it the screen says ' +
        'nothing about whether the work in front of a person is going, finished or waiting ' +
        'to be started.',
    ).toBeGreaterThanOrEqual(0);
    expect(
      title,
      'the run screen names no run in the hero step. The name of the section answers where in ' +
        'the application you stand; it does not answer which run you are looking at.',
    ).toBeGreaterThanOrEqual(0);
    expect(
      meta,
      'the run screen names no workspace under the title, so the head says which run it is and ' +
        'not where that run is doing its work.',
    ).toBeGreaterThanOrEqual(0);
    expect(
      state < title && title < meta,
      'the screen and the mockup disagree about the order of the head. The mockup draws the ' +
        'state line, then the title, then the metadata, and that order is what makes the ' +
        'title the loudest thing on the screen instead of the third thing under it.',
    ).toBe(true);
  });

  it('writes the title in the step the mockup writes it, read from both files', () => {
    /* 2026-09-01 — WYROCZNIĄ JEST `.rhead h1`, NIE `h1.sm`, i to jest przecelowanie, nie
       zluzowanie. `h1.sm` to stopień, którym makieta pisze tytuł EKRANU drugiego rzędu, i noszą
       go u niej trzy różne ekrany; nagłówek biegu zszedł na stopień jej `h2` (tytuł karty,
       panelu i okna dialogowego), bo bohaterem ekranu biegu jest praca, nie jej nagłówek.
       Reguła `.rhead h1` mówi o TYM nagłówku i tylko o nim, więc zmiana na innym ekranie
       makiety nie przewraca już tego punktu, a zmiana na tym — przewraca. */
    const wanted = property(ruleBody(html, '.rhead h1'), 'font-size');
    const mine = property(theme, '--text-title');

    expect(
      wanted,
      'nothing was read out of the `.rhead h1` rule in docs/mockup/index.html, so the ' +
        'comparison below would run between two empty strings and pass on nothing.',
    ).not.toBe('');
    expect(
      mine,
      'nothing was read out of `--text-title` in src/styles/theme.css, so the same comparison ' +
        'would pass on nothing from the other side.',
    ).not.toBe('');
    expect(
      mine,
      'the type ladder and the mockup disagree about the step of the run title. The mockup ' +
        'writes it at ' +
        wanted +
        ' and the ladder offers ' +
        mine +
        '. The class on the title below is only worth something while these two agree.',
    ).toBe(wanted);

    /* KLASA CZYTANA ZE ZNACZNIKA TYTUŁU, nie szukana w całym markupie. `toContain('text-title')`
       przechodzi, kiedy ten napis stoi gdziekolwiek indziej — a stoi: panel historii i karta
       agenta piszą nim swoje nagłówki. Pytanie brzmi „w jakim stopniu napisany jest TEN
       tytuł", więc odczyt idzie z jego własnego znacznika. */
    const tag = openingTag(runningScreen(), 'data-run-title');
    expect(
      tag,
      'the run head marks no element as the title of the run, so which step it is written in ' +
        'cannot be read at all and the comparison below would pass on an empty string.',
    ).not.toBe('');
    expect(
      tag,
      'the run title is not written in the step the mockup gives it. Any other step makes it ' +
        'either quieter than the metadata under it or louder than the work it stands over.',
    ).toContain('text-title');
  });

  it('gives the spend box the width the mockup gives it', () => {
    const wanted = property(ruleBody(html, '.spend'), 'width');

    expect(
      wanted,
      'nothing was read out of the `.spend` rule in docs/mockup/index.html, so the comparison ' +
        'below would pass on two empty strings.',
    ).not.toBe('');

    const markup = runningScreen();
    const box = /<div data-spend[^>]*style="([^"]*)"/.exec(markup)?.[1] ?? '';

    expect(
      box,
      'the run head renders no spend box with a declared width, so the amount this run has ' +
        'cost either is not on the screen or has no fixed place on it. A number that moves ' +
        'sideways whenever it grows a digit reads as a different number.',
    ).not.toBe('');
    expect(
      box.replace(/\s+/g, ''),
      'the screen and the mockup disagree about the width of the spend box. The mockup says ' +
        wanted +
        '.',
    ).toContain('width:' + wanted.replace(/\s+/g, ''));
  });

  it('stands inside the work area, above both of its columns', () => {
    const markup = runningScreen();
    /* `data-work="`, nie `data-work`: rząd kontrolek w pasku niesie `data-workflow-controls`
       i stoi wcześniej, więc szukanie samego przedrostka odpowiadałoby na pytanie o zupełnie
       inny element — a wtedy „nagłówek jest w obszarze pracy" przechodziłoby także dla
       nagłówka postawionego NAD nim. Zmierzone mutacją: bez cudzysłowu ten punkt był zielony
       po przeniesieniu nagłówka o piętro wyżej. */
    const work = markup.indexOf('data-work="');
    const head = markup.indexOf('data-run-head');
    const plan = markup.indexOf('data-plan-column');
    const stream = markup.indexOf('data-stream-column');

    expect(
      Math.min(work, head, plan, stream),
      'one of the four regions this point compares is not on the screen at all, so the ' +
        'ordering below would be comparing against a -1 and passing on nothing.',
    ).toBeGreaterThanOrEqual(0);
    expect(
      work < head && head < plan && head < stream,
      'the head of the run is not the first thing inside the work area. The mockup draws it as ' +
        'a band across the full width above both columns, and where it sits decides what it ' +
        'costs: everything above the work area is measured against the ceiling in ' +
        'docs/ARCHITECTURE.md §7, and 93 of those 96 pixels are already spent.',
    ).toBe(true);

    const tag = openingTag(markup, 'data-work');
    expect(
      tag,
      'the work area declares no style at all, so the head has no row to stand in.',
    ).not.toBe('');
    expect(
      tag.replace(/\s+/g, ''),
      'the work area declares no rows, so the head, the path of steps and the stream all land ' +
        'in tracks the browser makes up. The head takes the height of its content and the two ' +
        'columns take what is left; without that the columns share one row with it.',
    ).toContain('grid-template-rows');
  });

  it('heads nothing when there is no run and nothing that could be run', () => {
    const markup = renderToStaticMarkup(<Run />);

    expect(
      markup,
      'the screen carries no work area at all, so the point below would pass on markup that ' +
        'never mounted.',
    ).toContain('data-work');
    expect(
      markup,
      'the screen draws the head of a run when there is neither a run nor a workflow that ' +
        'could be started. A title over nothing promises something the screen will never put ' +
        'under it, and it costs its height every second it stands there.',
    ).not.toContain('data-run-head');
  });
});

/* Dwa punkty o samej REGULE, i mówią to o sobie: pytają, czy nagłówek liczy z tego, co przyszło,
 * a nie czy da się go zobaczyć. Widoczności dowodzą punkty wyżej, na zamontowanym ekranie. */
describe('what the head says is counted from what arrived, never filled in', () => {
  it('says when the run started only while the beginning is still in the window', () => {
    const facts = {
      workflow: WORKFLOW,
      nextUp: '',
      steps: STEPS,
      lines: [done(1, 3.41)],
      droppedBefore: 0,
      workspace: 'atlas',
      agents: 3,
      budgetUsd: 75,
    };

    expect(
      headlineFor(facts).eyebrow,
      'a run that is going, whose first line is still in the window, does not say when it ' +
        'started. That hour is the one fact on this screen a person uses to tell a run that ' +
        'has been stuck for an hour from one that started a minute ago.',
    ).toMatch(/^Running · started \d\d:\d\d$/u);

    expect(
      headlineFor({ ...facts, droppedBefore: 12 }).eyebrow,
      'the beginning of this run has fallen out of the window of lines, and the head still ' +
        'names an hour. The oldest line left is not the hour the run started, and nothing on ' +
        'the screen would say those are different things.',
    ).toBe('Running');
  });

  it('measures the meter against a ceiling, and draws none without one', () => {
    const facts = {
      workflow: WORKFLOW,
      nextUp: '',
      steps: STEPS,
      lines: [done(1, 3), done(2, 3)],
      droppedBefore: 0,
      workspace: 'atlas',
      agents: 3,
      budgetUsd: 24,
    };

    expect(
      headlineFor(facts).used,
      'six dollars spent against a ceiling of twenty-four is a quarter of it, and the bar ' +
        'beside the amount is the only thing on this screen that says how close the run is to ' +
        'the limit a person set.',
    ).toBeCloseTo(0.25);
    expect(
      headlineFor({ ...facts, budgetUsd: null }).used,
      'this run has no ceiling and the head still offers a fraction of one. A bar filled ' +
        'against a limit nobody set is a measurement of nothing.',
    ).toBeNull();
    expect(
      headlineFor({ ...facts, lines: [done(1, null)] }).used,
      'not one turn of this run reported a price and the head still draws a bar. An unknown ' +
        'cost shown as a full bar or an empty one both say something the numbers do not.',
    ).toBeNull();
  });
});
