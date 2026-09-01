/* Wiersz strumienia mówi KTO to zrobił i CO PO SOBIE ZOSTAWIŁ — a niepowodzenie widać.
 *
 * ZMIERZONA WADA (2026-08-18). `Tool` ginął na granicy sterownika, więc wiersze `read`, `search`,
 * `edit` i `ran` nie powstawały NIGDY; kiedy szew się domknął, okazało się, że wiersz rysuje
 * wszystko na szaro i jednym `<p>`. Trzy rzeczy z makiety nie miały gdzie wylądować i każda
 * niosła treść: podpis agenta w kolorze TOŻSAMOŚCI (`.ln .who`), prawa kolumna z metryką
 * (`.ln .m`, `+42 −8`) i blok wyjścia na lewej krawędzi w `--fail` (`.detail`). Czterech agentów
 * w jednym strumieniu było czterema identycznymi szarymi napisami.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: `expect(markup).toContain('var(--color-id-3)')`. Przechodzi dla
 * wiersza, który maluje KAŻDEGO agenta jednym, wpisanym z palca kolorem — czyli dla tej samej
 * wady z jednym hexem więcej. Odróżniają to dwie rzeczy: wartość oczekiwaną liczy
 * `identityToken()`, ta sama funkcja, z której żyje kwadrat na kafelku (niezmiennik 13), a obok
 * stoi asercja NEGATYWNA — na tym wierszu nie ma prawa pojawić się ani jeden token STANU
 * [DESIGN §3, „tożsamość ≠ stan"]. To jest jedyny sposób, w jaki ta reguła się psuje: nie przez
 * brak koloru, tylko przez ten sam.
 *
 * WIERSZE BUDUJE MODEL, nie ten plik. Metryka `+42 −8` powstaje w `feed/model.ts` z pól z drutu,
 * więc wpisana tutaj z palca przechodziłaby także wtedy, gdy komponent rysuje ją z czegoś innego.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { IDENTITY, STATUS, identityToken } from '../rail/colour';
import type { HistoryRow } from './model';
import { createFeed } from './model';
import { Line } from './line';

const FORGE = 'Forge';
const PATH = 'src/parser.rs';
const ADDED = 42;
const REMOVED = 8;

/**
 * Pełne wyjście polecenia, które padło. Ile z niego widać, rozstrzyga model.
 *
 * Numery dwucyfrowe z zerem wiodącym, bo `check-1` byłby PODCIĄGIEM `check-19` — asercja
 * negatywna o pierwszej linii przechodziłaby wtedy przez przypadek, na dowolnej implementacji.
 */
const OUTPUT = Array.from(
  { length: 30 },
  (_, at) => 'check-' + String(at + 1).padStart(2, '0') + ' did not pass',
);

/** Wiersze historii policzone przez MODEL — dokładnie te, które dostaje ekran. */
function rows(): readonly HistoryRow[] {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.edit(1, 0, FORGE, PATH, ADDED, REMOVED),
    line.ran(2, 4_000, FORGE, 'Ran the checks — they did not work', false, OUTPUT),
  ]);
  return feed.view.history;
}

const [changed, broke] = rows();

function markupOf(row: HistoryRow | undefined): string {
  if (row === undefined) return '';
  return renderToStaticMarkup(
    <Line
      row={row}
      onToggle={() => {
        /* Kryterium pyta o markup, nie o skutek; skutek ma swój własny plik. */
      }}
    />,
  );
}

/** Nazwy tokenów, po które ten markup naprawdę sięga. */
function tokensIn(markup: string): readonly string[] {
  return [...markup.matchAll(/var\((--[a-z0-9-]+)\)/g)].map((hit) => hit[1] ?? '');
}

const forgeRow = markupOf(changed);
const brokeRow = markupOf(broke);

describe('a line of the stream says who did it and what it left behind', () => {
  it('runs on rows the model really produced', () => {
    expect(
      [changed?.kind, broke?.kind],
      'the model produced something other than the two rows this file asks about, so every ' +
        'assertion below would be about a row nobody built.',
    ).toEqual(['edit', 'ran']);
    expect(changed?.metric, 'and the model, not this file, wrote the metric').toBe(
      '+' + String(ADDED) + ' −' + String(REMOVED),
    );
  });

  it('signs the line with the colour of the agent, taken from the one map that assigns it', () => {
    const wanted = identityToken(FORGE);

    expect(
      tokensIn(forgeRow),
      'the line does not reach for the identity colour of its agent at all. Four agents in one ' +
        'stream were four identical grey labels, so the only way to ask "who did this" was to ' +
        'read the letters.',
    ).toContain(wanted);
    expect(
      IDENTITY,
      'and the colour it reached for is an identity colour, dimmed — not something else that ' +
        'happens to be defined',
    ).toContain(wanted);
    expect(
      tokensIn(forgeRow).filter((name) => STATUS.includes(name)),
      'a state colour reached the line of an agent. Identity is never state [DESIGN §3]: the ' +
        'reference the earlier prototype gave the agent Forge exactly the hex that meant "waiting on you" ' +
        'on the tile next to it, and after that nobody trusted any colour on the screen.',
    ).toEqual([]);
    expect(forgeRow, 'the name of the agent is on the line as text, too').toContain(FORGE);
  });

  it('puts the number the change left behind in its own column', () => {
    expect(
      forgeRow,
      'the numbers from the wire (added, removed) have nowhere to land, which is what the whole ' +
        'right-hand column of the mockup exists for',
    ).toContain('+' + String(ADDED) + ' −' + String(REMOVED));
    expect(
      forgeRow.includes('<button'),
      'a change that has nothing more to show grew an expand control anyway. A control that ' +
        'opens nothing is a dead button with an extra step (invariant 16).',
    ).toBe(false);
  });

  it('shows the output of what failed, on the failure edge, and the end of it', () => {
    expect(
      brokeRow,
      'the run that failed carries no expand control, so its output cannot be reached at all',
    ).toContain('<button');
    expect(
      brokeRow,
      'output that failed is drawn like any other paragraph. The left edge in --fail is the ' +
        'only thing that tells the two apart [makieta .detail].',
    ).toContain('border-l-fail');
    expect(
      brokeRow,
      'the LAST lines of the output are the half that carries the reason; slice(0, 20) shows ' +
        'the beginning of the log, which never does, and passes any check that counts rows.',
    ).toContain(OUTPUT.at(-1) ?? '');
    expect(
      brokeRow.includes(OUTPUT[0] ?? 'check-01'),
      'and the beginning of a thirty-line log is past the twenty the model handed over',
    ).toBe(false);
  });
});
