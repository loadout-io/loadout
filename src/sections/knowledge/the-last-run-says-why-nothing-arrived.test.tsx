/* Pusta kolejka decyzji ma POWÓD, kiedy tura po biegu nie zdążyła odpowiedzieć.
 *
 * ZMIERZONA WADA (bieg meetnotes, 2026-09-04). Dwie godziny pracy, 57,52 USD, dziewięć
 * przekazań, „Learn from this run" włączone — i tura, która to wszystko miała przeczytać,
 * dostała osiem centów. Zeszła na nich po 22 sekundach, nie zostawiła ani jednej notatki, a ten
 * ekran wyglądał wtedy DOKŁADNIE tak samo, jak po biegu, z którego nie było się czego nauczyć.
 * Cisza jest nieodróżnialna od awarii, a ktoś za tę turę zapłacił.
 *
 * # Trzy słabe wersje tego kryterium
 *
 * **Pierwsza: zapytać `ranOutOfSomething` wprost.** Zwrócona wartość dowodzi, że mechanizm
 * istnieje; markup ekranu dowodzi, że produkt działa (niezmiennik 29). Dlatego niżej renderuje
 * się `<App section="knowledge" />`, czyli cała powłoka przez odkrywanie ekranów — tak samo jak
 * w `the-queue-is-the-hero.test.tsx` obok.
 *
 * **Druga: sprawdzić samą OBECNOŚĆ zdania w markupie.** Przechodzi ją `title="…"`, czyli
 * podpowiedź, której nikt nie zobaczy. Zdanie jest więc sądzone jako WĘZEŁ TEKSTOWY.
 *
 * **Trzecia: sprawdzić tylko ekran pełny.** Bieg, którego tura zeszła na cenie, NIE ZOSTAWIA
 * notatek — więc na świeżym projekcie ten ekran jest pusty i to właśnie tam brak wyjaśnienia
 * boli najbardziej. Dlatego oba stany ekranu, pełny i pusty, są tu osobnymi przypadkami.
 *
 * Że ta sama kwota i to samo zdanie wychodzą z `run.json` na granicę, sądzą po tamtej stronie
 * `src-tauri/tests/it/z38_reflection_says_it_ran_out.rs` i `…/z38_the_window_reads_the_ceiling.rs`;
 * bez nich wszystko poniżej stoi na fiksturze.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';

import { App } from '../../App';
import type { Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { useSkills } from '../../state/skills';

/** Sufit, który obowiązywał bieg z audytu: jeden procent z 57,52 USD. */
const CEILING_USD = 0.58;

/** Zdanie, które ma zobaczyć człowiek — wypisane słowo w słowo (niezmiennik 20). */
const RAN_OUT_OF_MONEY =
  "Learn from this run didn't finish: the note-taker used its $0.58 before answering.";

/** I to samo, kiedy turę zdjął zegar, a nie cena. */
const RAN_OUT_OF_TIME =
  "Learn from this run didn't finish: the note-taker ran out of time before answering.";

/** Notatka, która już jedzie do promptów — żeby ekran pełny miał co narysować. */
const IN_USE: Note = {
  place: 'library',
  id: 'in-use',
  title: 'Say what changed',
  rule: 'Say what changed, not what you tried.',
  because: 'Reports without it needed a second read every time.',
  status: 'in-use',
  scope: 'everywhere',
  length: 96,
  occurrences: 11,
  modified: '2026-08-30T17:40:00Z',
};

/** Czy to zdanie stoi w markupie jako TEKST, a nie wyłącznie jako wartość atrybutu. */
function readsAsText(markup: string, sentence: string): boolean {
  return markup.includes('>' + sentence + '<');
}

/** Zdanie, które ten ekran postawił w wierszu o ostatniej turze — albo pusty napis. */
function stoppedRow(markup: string): string {
  const at = markup.indexOf('data-learning-stopped');
  if (at < 0) return '';
  const opens = markup.indexOf('>', at);
  const closes = markup.indexOf('<', opens);
  return opens < 0 || closes < 0 ? '' : markup.slice(opens + 1, closes);
}

/** Markup tak, jak czyta go człowiek: React zapisuje apostrof i `&` jako encje. */
function readable(markup: string): string {
  return markup
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
}

function screen(): string {
  return readable(renderToStaticMarkup(<App section="knowledge" />));
}

beforeEach(() => {
  useMemory.setState({
    notes: [],
    passed: [],
    message: null,
    passedProblem: null,
    lastLearning: null,
    choice: null,
    pendingDiscard: null,
    read: true,
  });
  useSkills.setState({
    installed: [],
    pending: null,
    adding: null,
    acknowledged: [],
    message: null,
    folders: 'read',
    removing: null,
  });
});

describe('the empty queue says why, when the turn that fills it ran out', () => {
  it('says it on the screen a person opens after that run, with the amount', () => {
    useMemory.setState({
      notes: [IN_USE],
      lastLearning: {
        ran: false,
        kept: 0,
        discardedAgain: 0,
        droppedWithoutReason: 0,
        why: 'ran-out-of-budget',
        budgetUsd: CEILING_USD,
      },
    });
    const markup = screen();

    expect(
      stoppedRow(markup),
      'the last run paid for a turn that ran out of money before it answered, and this screen — ' +
        'the one place where the notes from that turn were supposed to land — says nothing ' +
        'about it. An empty queue then reads as "nothing was worth keeping", which is not what ' +
        'happened',
    ).toBe(RAN_OUT_OF_MONEY);
    expect(
      readsAsText(markup, RAN_OUT_OF_MONEY),
      'the sentence is in the markup but not as text a person reads: a title attribute answers ' +
        'only somebody who already stopped the mouse there',
    ).toBe(true);
  });

  it('says it on the empty screen too, which is where that run leaves a person', () => {
    useMemory.setState({
      lastLearning: {
        ran: false,
        kept: 0,
        discardedAgain: 0,
        droppedWithoutReason: 0,
        why: 'ran-out-of-time',
      },
    });
    const markup = screen();

    expect(
      markup.includes('data-empty'),
      'this case is about the screen that has nothing on it yet, and this markup already has ' +
        'shelves — so it would be asking its question of the wrong screen',
    ).toBe(true);
    expect(
      stoppedRow(markup),
      'a run whose turn was stopped by the clock leaves exactly the screen this case renders: ' +
        'no notes, no explanation, and an invitation saying nothing is here yet. The invitation ' +
        'is true and useless — the person needs to know the turn went and did not finish',
    ).toBe(RAN_OUT_OF_TIME);
  });

  it('stays quiet about a run whose turn did its work', () => {
    useMemory.setState({
      notes: [IN_USE],
      lastLearning: {
        ran: true,
        kept: 2,
        discardedAgain: 0,
        droppedWithoutReason: 0,
      },
    });
    expect(
      stoppedRow(screen()),
      'the screen explains itself over a run that has nothing to explain. "It kept two notes" ' +
        'is already said by the queue itself, and a second sentence about it is a second answer ' +
        'to one question (invariant 13)',
    ).toBe('');

    useMemory.setState({ lastLearning: null });
    expect(
      stoppedRow(screen()),
      'a project whose runs say nothing about a private turn — an older record, or no run at ' +
        'all — still gets a sentence about one. That is inventing an answer out of a missing ' +
        'key (invariant 17)',
    ).toBe('');
  });
});
