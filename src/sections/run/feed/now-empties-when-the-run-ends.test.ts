/* Koniec biegu opróżnia strefę TERAZ.
 *
 * ZMIERZONA WADA, NIEZALEŻNA OD TEJ Z OKNA I STARSZA. `runEnded` jest aliasem na `unpark`:
 * gasi `parked` i `toCarry`, a mapy `doing` nie tyka. Więc po zakończeniu biegu ostatnie zdanie
 * każdego agenta zostaje w strefie „co się dzieje teraz" na zawsze — cztery wiersze o pracy,
 * której nikt nie wykonuje, w jednym z dwóch regionów, którym ARCHITECTURE §7 pozwala się
 * ruszać (niezmiennik 17). Człowiek patrzy w to miejsce właśnie po to, żeby wiedzieć, czy
 * cokolwiek żyje.
 *
 * DLACZEGO POPRAWKA JEST W MODELU, A NIE W KOMPONENCIE. `now.tsx` dostaje od wołającego osobny
 * fakt „bieg naprawdę żyje" i trzyma za nim pulsującą kropkę oraz nagłówek. Dołożenie tam
 * jeszcze jednego sita nad `now.rows` zgasiłoby objaw i zostawiło `doing` pełne widm dla
 * następnego czytającego, a tym czytającym jest już `../rail/roster.ts`. Kuracja mieszka
 * w mapowaniu zdarzenie→linia (niezmiennik 15), a „kto pracuje" ma jedną odpowiedź w jednym
 * miejscu (niezmiennik 13).
 *
 * SŁABA WERSJA: test na samym „po `runEnded()` jest pusto". Przechodzi dla implementacji, która
 * czyści `doing` przy KAŻDEJ paczce — a wtedy strefa trzyma najwyżej to, co przyszło ostatnią
 * paczką, i odpowiada na pytanie „kto powiedział coś ostatni" zamiast „kto pracuje". Rozróżnia
 * to przypadek następnego biegu: dwie paczki, dwóch agentów, obaj dalej w strefie. Drugie sito
 * to historia — koniec biegu kasuje strefę stanu, nigdy zapisu tego, co się stało.
 */
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const FORGE = 'Forge';
const NEEDLE = 'Needle';

const FORGE_SAID = 'Rewriting the splitter.';
const NEEDLE_SAID = 'Checking the header row.';

/** Zdania z NASTĘPNEGO biegu — kontrola przeciw naprawie przez wyłączenie strefy. */
const FORGE_AGAIN = 'Starting on the splitter again.';
const NEEDLE_AGAIN = 'Reading the second file.';

/** Pytanie i odpowiedź — tylko po to, żeby było co zgasić poza samą strefą. */
const QUESTION = 'Does the plan look right?';
const ANSWER = 'Yes, keep the old splitter behind a flag.';

/**
 * Bieg, który naprawdę idzie: dwóch agentów powiedziało po zdaniu, jeden z nich właśnie myśli.
 *
 * Dwóch, nie jednego, i dwie paczki, nie jedna: strefa TERAZ ma trzymać po jednym wierszu na
 * agenta biegu, a scena z jednym agentem nie odróżnia tego od „trzymam ostatnią paczkę".
 */
function running() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, FORGE, FORGE_SAID), line.note(2, 500, NEEDLE, NEEDLE_SAID)]);
  feed.appendLines([line.thinking(3, 1_000, FORGE)]);
  return feed;
}

describe('the end of a run empties the NOW zone', () => {
  it('has nobody doing anything once the run is gone', () => {
    const feed = running();

    expect(
      feed.view.now.rows.map((row) => row.agent),
      'both agents stand in the zone while the run is going. Without that, the emptiness below ' +
        'is the emptiness of an empty scene and proves nothing.',
    ).toEqual([FORGE, NEEDLE]);

    feed.runEnded();

    expect(
      feed.view.now.rows,
      'a run that is gone has nobody working. Left standing, the last sentence of every agent ' +
        'sits in "what is happening now" until you close the app, and it reads exactly like ' +
        'work in flight — a relation the data does not have (invariant 17).',
    ).toEqual([]);
  });

  it('puts out Thinking… with it', () => {
    const feed = running();
    expect(feed.view.now.thinking, 'somebody is thinking while the run is going').toBe(FORGE);

    feed.runEnded();

    expect(
      feed.view.now.thinking,
      'nobody is thinking once the run is gone. A slot left live says somebody is at work when ' +
        'the work is over, and it is the last thing on this screen a person would doubt.',
    ).toBeNull();
  });

  it('leaves the history untouched', () => {
    const feed = running();
    const before = feed.view.history;

    feed.runEnded();

    expect(
      feed.view.history,
      'the end of a run clears the zone that says what is happening, never the record of what ' +
        'happened. And it is the SAME array: a fresh one asks React to redraw the whole story ' +
        'for something that never entered it.',
    ).toBe(before);
    expect(
      feed.view.history.map((row) => row.label),
      'both sentences are still readable after the run is over',
    ).toEqual([FORGE_SAID, NEEDLE_SAID]);
  });

  it('fills the zone again on the next run, one row per agent across batches', () => {
    const feed = running();
    feed.runEnded();

    feed.appendLines([line.note(4, 10_000, FORGE, FORGE_AGAIN)]);
    feed.appendLines([line.note(5, 10_500, NEEDLE, NEEDLE_AGAIN)]);

    expect(
      feed.view.now.rows,
      'two batches, two agents, both still there. Clearing the map on every batch empties the ' +
        'zone just as well and leaves it holding whatever arrived last, so it answers "who ' +
        'spoke most recently" instead of "who is working" — and switching the zone off for ' +
        'good passes both of those and deletes it for the runs that really are going.',
    ).toEqual([
      { agent: FORGE, text: FORGE_AGAIN },
      { agent: NEEDLE, text: NEEDLE_AGAIN },
    ]);
  });

  it('still clears the two things it clears today', () => {
    const feed = running();
    feed.appendLines([line.asked(4, 2_000, FORGE, QUESTION, [])]);
    expect(feed.view.parked, 'the question stops the run and says so in its own field').toBe(true);

    feed.answer(4, ANSWER);
    expect(feed.view.toCarry, 'and the sentence is queued for the agent').toBe(ANSWER);

    feed.runEnded();

    expect(
      feed.view.parked,
      'a run that is gone is not standing on anyone’s question, so the control that lets it ' +
        'through goes with it. This is what the end of a run already does today, and emptying ' +
        'the zone must not cost it.',
    ).toBe(false);
    expect(feed.view.toCarry, 'and there is nobody left to carry the sentence to').toBe('');
  });
});
