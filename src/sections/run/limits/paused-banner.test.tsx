/* Kryterium 8 dla T-21: pasek pauzy pokazuje godzinę LOKALNĄ, jeden raz na bieg.
 *
 * Słaba wersja tego kryterium to `expect(markup).toContain('3:30 PM')`. Przechodzi ją tekst
 * wpisany na sztywno — i taki tekst pokazuje 3:30 PM także komuś, kto siedzi w Londynie.
 *
 * Rozstrzygają dwie rzeczy naraz: dwie strefy dające dwa RÓŻNE wyniki oraz asercja, że surowa
 * liczba 1786800600 nie pojawia się w markupie. Razem wykluczają i stałą, i wyświetlenie epoki
 * albo ISO — „resets at 1786800600" jest gorsze niż brak paska, bo wygląda na odpowiedź.
 *
 * Bez jsdom: `renderToStaticMarkup` z `react-dom/server` (patrz nagłówek `at-once.test.tsx`).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { PausedBanner } from './paused-banner';
import type { RunView } from './paused-banner';

/** Kiedy limit wraca — dosłownie z `docs/research/fixtures/claude-stream.jsonl`. */
const RESETS_AT = 1786800600;

/** Bieg, który czeka, i ma przy tym trzy trwające kroki. */
const WAITING: RunView = {
  waitingUntil: RESETS_AT,
  steps: ['running', 'running', 'running'],
};

/** Ten sam bieg, kiedy wysyła. */
const SENDING: RunView = {
  waitingUntil: null,
  steps: ['running', 'running', 'running'],
};

/* Dwie spacje nierozdzielające, zapisane numerem, a nie samym znakiem: w pliku źródłowym
 * różnica między nimi a zwykłą spacją jest niewidoczna dla oka i dla recenzji. */
const NARROW_NO_BREAK_SPACE = String.fromCharCode(0x202f);
const NO_BREAK_SPACE = String.fromCharCode(0x00a0);

/*
 * ICU od wersji 72 stawia przed AM/PM wąską spację nierozdzielającą, a nie zwykłą, i Node
 * bierze godzinę właśnie z ICU. Sprowadzamy odstępy do jednego kształtu, bo to kryterium jest
 * o GODZINIE i o STREFIE: przypięcie akurat tej odmiany spacji zamieniłoby poprawny komponent
 * w czerwony przy najbliższej aktualizacji biblioteki, nie mówiąc przy tym nic o tym, co widzi
 * człowiek.
 */
function plainSpaces(text: string): string {
  return text.replaceAll(NARROW_NO_BREAK_SPACE, ' ').replaceAll(NO_BREAK_SPACE, ' ');
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function markupFor(run: RunView, zone: string): string {
  return plainSpaces(renderToStaticMarkup(<PausedBanner run={run} zone={zone} />));
}

describe('the bar that says the run is waiting for the limit to come back', () => {
  it('says when it comes back, in the hour the reader keeps', () => {
    expect(
      markupFor(WAITING, 'Europe/Warsaw'),
      'the sentence names the hour on this reader clock and says what is being waited for',
    ).toContain('Waiting for your Claude usage to reset at 3:30 PM.');
  });

  it('says a different hour somewhere else', () => {
    const warsaw = markupFor(WAITING, 'Europe/Warsaw');
    const elsewhere = markupFor(WAITING, 'UTC');

    expect(
      elsewhere,
      'the same instant is a different hour two zones away, and a sentence typed in by hand ' +
        'cannot tell the two apart',
    ).toContain('Waiting for your Claude usage to reset at 1:30 PM.');
    expect(elsewhere, 'so the two renderings have to differ').not.toBe(warsaw);
  });

  it('never puts the machine number on the screen', () => {
    const markup = markupFor(WAITING, 'Europe/Warsaw');
    expect(markup, 'the number from the wire is not something a person reads').not.toContain(
      '1786800600',
    );
    expect(markup, 'and neither is the machine spelling of the same instant').not.toContain(
      'T13:30',
    );
  });

  it('shows one bar for the run, not one per step', () => {
    expect(
      occurrences(markupFor(WAITING, 'Europe/Warsaw'), 'data-paused-banner'),
      'three steps are running and the run is still waiting for exactly one thing, so there ' +
        'is exactly one live place on the screen saying so',
    ).toBe(1);
  });

  it('renders nothing at all while the run is sending', () => {
    expect(
      markupFor(SENDING, 'Europe/Warsaw'),
      'a run that is sending has no bar — not an empty bar, no element. An empty one holds ' +
        'its space and teaches people to stop reading that part of the screen',
    ).toBe('');
  });
});
