/* AC-2 dla T-115: prawdziwa sekcja Run pokazuje sumę wydatków obu vendorów.
 *
 * SŁABA WERSJA woła `spendFor()` albo `stripFor()` wprost. Obie funkcje już umieją policzyć
 * koszt i taka specyfikacja przechodzi nad dzisiejszym defektem: produkcyjny `index.tsx` woła
 * `stripFor(run.workflow, run.steps)` bez wydatku, więc człowiek nie widzi ani centa.
 * Dlatego niżej renderujemy cały ekran i wycinamy chip z jego markupu.
 *
 * DWA PŁATNE WIERSZE są treścią wyroczni. Jeden wiersz zazieleni implementację pokazującą
 * pierwszy albo ostatni koszt zamiast sumy — dokładnie lukę, przez którą T-102 przeszło zielono.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { FeedLine, Step } from '../../../state/run';
import { useRun } from '../../../state/run';
import Run from '../index';
import { setBudgetUsd } from '../limits/chosen';

const CODEX_COST = 0.41;
const CLAUDE_COST = 0.79;
const TOTAL = '$1.20';

const STEPS: readonly Step[] = [
  { id: 'code', name: 'Code', state: 'succeeded' },
  { id: 'check', name: 'Check', state: 'succeeded' },
];

function done(id: number, agent: string, costUsd: number | null): FeedLine {
  return {
    kind: 'done',
    agent,
    text: 'Done',
    turns: 1,
    durationMs: 1_000,
    costUsd,
    inputTokens: 10_000,
    outputTokens: 20_000,
    cachedTokens: 5_000,
    ended: 'well',
    id,
    at: id * 1_000,
  };
}

/** Tekst jedynego chipu wydatku zamontowanego w produkcyjnym pasku. */
function spendChip(markup: string): string {
  const hit = /<span data-copyable[^>]*>([^<]*)<\/span>/.exec(markup);
  return hit?.[1] ?? '';
}

function seed(lines: readonly FeedLine[]): void {
  useRun.setState({
    workflow: 'Both vendors',
    steps: STEPS,
    lines,
  });
}

describe('the mounted Run screen sums what both vendors cost', () => {
  it('shows neither operand, but their sum, with and without a limit', () => {
    seed([done(1, 'Code', CODEX_COST), done(2, 'Check', CLAUDE_COST)]);

    setBudgetUsd(null);
    const withoutLimit = spendChip(renderToStaticMarkup(<Run />));
    expect(
      withoutLimit,
      'the production Run screen mounted no spend chip. Calling spendFor() directly would ' +
        'miss exactly this broken wire from the feed to index.tsx.',
    ).not.toBe('');
    expect(
      withoutLimit,
      'the two paid rows cost $0.41 and $0.79, so the screen has to show their $1.20 sum.',
    ).toContain(TOTAL);
    expect(
      withoutLimit,
      "showing only the first paid row is not a total and was one of T-102's false greens.",
    ).not.toContain('$' + CODEX_COST.toFixed(2));
    expect(
      withoutLimit,
      'showing only the last paid row is not a total and was the other false-green shape.',
    ).not.toContain('$' + CLAUDE_COST.toFixed(2));
    expect(
      withoutLimit,
      'a run without a limit has no honest denominator, so the chip must not invent one.',
    ).not.toContain(' of ');

    setBudgetUsd(5);
    const withLimit = spendChip(renderToStaticMarkup(<Run />));
    expect(
      withLimit,
      'setting a real limit changes only the denominator; the numerator is still both rows.',
    ).toContain(TOTAL + ' of $5');
  });

  it('does not turn two unknown prices into a made-up $0.00', () => {
    setBudgetUsd(null);
    seed([done(3, 'Code', null), done(4, 'Check', null)]);

    const markup = renderToStaticMarkup(<Run />);
    expect(
      markup,
      'two vendors that reported no price did not report zero. $0.00 would be a measured-looking ' +
        'number made from missing data.',
    ).not.toContain('$0.00');
  });
});
