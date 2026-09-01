/* Nagłówek strumienia mówi, że jest żywy, wymienia tych, którzy się odezwali, i pozwala go
 * zawęzić do jednego z nich.
 *
 * # Po co
 *
 * Makieta `polecenie.html` (`.sthead`) stawia nad strumieniem cztery rzeczy: pomarańczową
 * kropkę z napisem `Live`, chipy z podpisami agentów (aktywny wypełniony), i po prawej zieloną
 * kropkę z napisem `following`. Bez nich strumień czterech agentów jest jedną kolumną, w której
 * nie da się odczytać wątku jednego z nich, i nie ma czym powiedzieć, czy widok jeszcze nadąża.
 *
 * # Dlaczego chipy pochodzą Z DANYCH, a nie z listy wpisanej w widoku
 *
 * Niezmiennik 17: okno nie rysuje relacji, których nie ma w danych. Chip `Needle` nad biegiem,
 * w którym Needle się nie odezwał, obiecuje wątek, którego nie ma — a kliknięty daje pusty
 * strumień i czyta się jak zepsuta aplikacja. Dlatego lista podpisów jest LICZONA z historii,
 * a kryterium sprawdza obie strony: że kto mówił, ma chip, i że kto nie mówił, chipa nie ma.
 *
 * # Co to kryterium dowodzi, a czego nie
 *
 * Dowodzi, że nagłówek stoi na prawdziwej ścieżce (markup produkcyjnego `Feed`) i że polityka
 * zawężania odpowiada prawdziwie. **Nie** dowodzi, że kliknięcie w chip dochodzi — to repo nie
 * ma jsdom, więc `onClick` nie odpala się tu ani razu. Tamtą połowę bierze na siebie
 * `e2e/tests/a-key-answers-and-a-chip-narrows.spec.ts`, prawdziwą myszą w prawdziwym chromium
 * (niezmiennik 29: kryterium wolno wybrać jedno z trzech, ale nie wolno poprzestać na
 * wartości zwróconej przez funkcję, której nikt nie woła).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Feed } from './feed';
import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import type { FeedView, HistoryRow } from './model';
import { createFeed } from './model';
import { EVERYONE, onlyFrom, speakersIn } from './speakers';

const SCOUT = 'Scout';
const BUILDER = 'Builder';
const NEEDLE = 'Needle';

const SCOUT_SAID = 'Read eighteen files under the migration path.';
const BUILDER_SAID = 'Reused the open connection instead of opening a second one.';

function markupOf(view: FeedView): string {
  return renderToStaticMarkup(
    <Feed
      view={view}
      portRef={() => {
        /* Przewijanie ma swój własny plik. */
      }}
      onToggle={() => {
        /* Rozwijanie wiersza też. */
      }}
      onAnswer={() => {
        /* Odpowiadanie ma swój plik. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza to inna kontrolka. */
      }}
    />,
  );
}

/** Strumień, w którym odezwało się dwoje z trojga agentów biegu. */
function twoOfThreeSpoke(): FeedView {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 1_000, SCOUT, SCOUT_SAID)]);
  feed.appendLines([line.note(2, 2_000, BUILDER, BUILDER_SAID)]);
  feed.appendLines([line.note(3, 3_000, SCOUT, 'And it fails the same way every time.')]);
  return feed.view;
}

/** Nazwy chipów podpisu w markupie, w kolejności wystąpienia. */
function chipNames(markup: string): readonly string[] {
  return [...markup.matchAll(/<button[^>]*\bdata-speaker="([^"]*)"[^>]*>/g)].map(
    (hit) => hit[1] ?? '',
  );
}

describe('the head of the stream says it is live and narrows it to one voice', () => {
  it('says the stream is live, and says the view is following it', () => {
    const markup = markupOf(twoOfThreeSpoke());

    expect(
      markup,
      'the mockup puts a beating dot and the word Live over this column, because a stream that ' +
        'has stopped and a stream nobody has spoken into for a minute look exactly the same. ' +
        'Markup: ' +
        markup.slice(0, 400),
    ).toContain('Live');
    expect(
      markup,
      'and it says on the right that the view is FOLLOWING the newest line. Without it, a ' +
        'person who scrolls up has nothing on screen telling them why new lines stopped ' +
        'appearing at the bottom',
    ).toContain('following');
  });

  it('mints one chip per voice that really spoke, and none for the rest', () => {
    const view = twoOfThreeSpoke();

    expect(
      speakersIn(view.history),
      'the chips are counted from the stream, in the order the voices first spoke — not from a ' +
        'list written into the view. Scout spoke twice and gets ONE chip',
    ).toEqual([SCOUT, BUILDER]);

    const names = chipNames(markupOf(view));
    expect(
      names,
      'the head has to carry the chip that clears the narrowing plus one per voice. It ' +
        'rendered: ' +
        JSON.stringify(names),
    ).toEqual([EVERYONE, SCOUT, BUILDER]);
    expect(
      names.includes(NEEDLE),
      'a chip for an agent who never said anything promises a thread that does not exist ' +
        '(invariant 17), and clicking it gives an empty stream — which reads as a broken app',
    ).toBe(false);
  });

  it('marks which chip is on, so the head says what you are looking at', () => {
    const markup = markupOf(twoOfThreeSpoke());
    const on = /<button[^>]*\bdata-speaker="([^"]*)"[^>]*\baria-pressed="true"/.exec(markup);

    expect(
      on?.[1],
      'nothing in the head says which chip is the one in force. Four chips that all look the ' +
        'same cannot answer "am I seeing everything, or one voice" — and that question is the ' +
        'whole reason the row exists. Markup: ' +
        markup.slice(0, 500),
    ).toBe(EVERYONE);
  });

  it('narrows the stream to one voice, and clearing it brings the others back', () => {
    const view = twoOfThreeSpoke();
    const said = (rows: readonly HistoryRow[]): readonly string[] => rows.map((row) => row.label);

    expect(
      said(onlyFrom(view.history, SCOUT)),
      'narrowing to one voice has to leave that voice ALONE on screen. A filter that keeps ' +
        'everything is a control with no effect, which is worse than no control (invariant 16)',
    ).toEqual([SCOUT_SAID, 'And it fails the same way every time.']);
    expect(
      said(onlyFrom(view.history, BUILDER)),
      'and narrowing to the other voice leaves the other one',
    ).toEqual([BUILDER_SAID]);
    expect(
      said(onlyFrom(view.history, EVERYONE)),
      'the chip that clears the narrowing has to bring every line back, in its own order — ' +
        'otherwise there is no way back to the whole run once you have looked at one agent',
    ).toEqual([SCOUT_SAID, BUILDER_SAID, 'And it fails the same way every time.']);
  });

  it('says nothing about voices when nobody has spoken', () => {
    const feed = createFeed(sealedScroller());

    expect(
      speakersIn(feed.view.history),
      'an empty stream has no voices, so the head has no chips to draw — a row of chips over ' +
        'an empty stream is furniture that promises a run that has not happened',
    ).toEqual([]);
    expect(
      chipNames(markupOf(feed.view)),
      'and the empty screen stays the invitation it is, with no filter row over it',
    ).toEqual([]);
  });
});
