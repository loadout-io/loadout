/* T-162: strefa TERAZ trzyma wyłącznie pracę, która TRWA — nie ostatnie zdanie każdego, kto
 * kiedykolwiek się odezwał.
 *
 * ZMIERZONA WADA, z pierwszego długiego Murmura-1 (`docs/PLAN.md` §6d): „NOW trzyma zakończonych
 * agentów". `doing` w `feed/model.ts` jest tylko DOPISYWANE — wpis powstaje przy pierwszej linii
 * agenta i nie znika, dopóki nie skończy się CAŁY bieg. Przy grafie na kilkanaście kroków znaczy
 * to, że po dziesięciu minutach strefa „co się dzieje teraz" wymienia dziesięciu agentów,
 * z których pracuje jeden. To jest relacja, której w danych nie ma (niezmiennik 17), w jednym
 * z dwóch regionów, którym ARCHITECTURE §7 pozwala się ruszać.
 *
 * `runEnded()` załatwił KONIEC biegu (`now-empties-when-the-run-ends.test.ts`) i to jest inna
 * wada niż ta: tam zostawało wszystko po zakończeniu, tutaj zostaje w trakcie.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: „po `succeeded` agenta nie ma w strefie". Przechodzi dla
 * implementacji, która kasuje agenta na KAŻDY wiersz `stepState` — a wtedy `running` też go
 * wyrzuca i strefa jest pusta przez cały bieg. Przechodzi też dla implementacji, która kasuje
 * agenta przy pierwszym zakończonym kroku, choć ten agent biegnie w dwóch kopiach naraz i drugą
 * dalej pracuje. Rozróżniają to dwa przypadki niżej: `running` niczego nie zdejmuje, a agent
 * z dwiema żywymi kopiami zostaje po zakończeniu pierwszej.
 */
import { describe, expect, it } from 'vitest';

import { line } from './fixtures/lines';
import { sealedScroller } from './fixtures/scroller';
import { createFeed } from './model';

const FORGE = 'Forge';
const NEEDLE = 'Needle';
const BUILD = 's_build';
const SECOND_COPY = 's_build_2';
const CHECK = 's_check';

/** Dwóch agentów w robocie, po zdaniu każdy. Dwóch, bo jeden nie odróżnia sita od pustki. */
function running() {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.note(1, 0, FORGE, 'Rewriting the splitter.'),
    line.note(2, 500, NEEDLE, 'Checking the header row.'),
  ]);
  return feed;
}

function agentsInTheZone(feed: ReturnType<typeof createFeed>): string[] {
  return feed.view.now.rows.map((row) => row.agent);
}

describe('the NOW zone holds only work that is still going', () => {
  it('drops an agent whose step is over and keeps the one still working', () => {
    const feed = running();
    expect(
      agentsInTheZone(feed),
      'both agents work at the start. Without this the emptiness below proves nothing.',
    ).toEqual([FORGE, NEEDLE]);

    feed.appendLines([line.stepState(3, 1_000, FORGE, BUILD, 'running')]);
    expect(
      agentsInTheZone(feed),
      'a step that STARTED must not empty the zone — that would leave it blank all run long',
    ).toEqual([FORGE, NEEDLE]);

    feed.appendLines([line.stepState(4, 2_000, FORGE, BUILD, 'succeeded')]);
    expect(
      agentsInTheZone(feed),
      'the agent whose only step finished is no longer doing anything, and the other one is',
    ).toEqual([NEEDLE]);
  });

  it('keeps an agent that still has another copy running', () => {
    const feed = running();
    feed.appendLines([
      line.stepState(3, 1_000, FORGE, BUILD, 'running'),
      line.stepState(4, 1_100, FORGE, SECOND_COPY, 'running'),
    ]);

    feed.appendLines([line.stepState(5, 2_000, FORGE, BUILD, 'succeeded')]);

    expect(
      agentsInTheZone(feed),
      'one copy of this agent finished and the other is still at work, so the agent is still ' +
        'working. Keying the zone by agent alone loses that.',
    ).toEqual([FORGE, NEEDLE]);
  });

  it('lets an agent come back when its next step starts', () => {
    const feed = running();
    feed.appendLines([
      line.stepState(3, 1_000, FORGE, BUILD, 'running'),
      line.stepState(4, 2_000, FORGE, BUILD, 'succeeded'),
    ]);
    expect(agentsInTheZone(feed), 'the agent left after its step finished').toEqual([NEEDLE]);

    feed.appendLines([line.note(5, 3_000, FORGE, 'Starting the checks.')]);

    expect(
      agentsInTheZone(feed),
      'the same agent picked up the next step and is working again',
    ).toEqual([NEEDLE, FORGE]);
  });

  it('treats every way a step can end as an ending', () => {
    for (const over of ['succeeded', 'failed', 'cancelled', 'skipped']) {
      const feed = running();
      feed.appendLines([
        line.stepState(3, 1_000, FORGE, CHECK, 'running'),
        line.stepState(4, 2_000, FORGE, CHECK, over),
      ]);
      expect(agentsInTheZone(feed), `a step that ended as ${over} is not work in flight`).toEqual([
        NEEDLE,
      ]);
    }
  });

  it('leaves the history alone', () => {
    const feed = running();
    const before = feed.view.history.length;

    feed.appendLines([
      line.stepState(3, 1_000, FORGE, BUILD, 'running'),
      line.stepState(4, 2_000, FORGE, BUILD, 'succeeded'),
    ]);

    expect(
      feed.view.history.length,
      'the zone is state, the history is the record — a finished step never unsays what was said',
    ).toBe(before);
    expect(
      feed.view.history.some((row) => row.agent === FORGE),
      'what the finished agent said is still in the record',
    ).toBe(true);
  });

  it('does not turn a carried-on failure back into live work or history', () => {
    const feed = running();
    const before = feed.view.history.length;
    feed.appendLines([
      line.stepState(3, 1_000, FORGE, BUILD, 'running'),
      line.stepState(4, 2_000, FORGE, BUILD, 'failed'),
      line.stepCarriedOn(5, 2_001, FORGE, BUILD),
    ]);

    expect(
      agentsInTheZone(feed),
      'the carry-on fact describes an already failed step; it must not resurrect the agent',
    ).toEqual([NEEDLE]);
    expect(
      feed.view.history.length,
      'stepCarriedOn is scheduler state for the tile, not a sentence in the run history',
    ).toBe(before);
  });

  it('closes live work from the single self-contained carry-on outcome', () => {
    const feed = running();
    const before = feed.view.history.length;
    feed.appendLines([
      line.stepState(3, 1_000, FORGE, BUILD, 'running'),
      line.stepCarriedOn(4, 2_000, FORGE, BUILD),
    ]);

    expect(
      agentsInTheZone(feed),
      'stepCarriedOn is the whole failed terminal outcome, so it must close the live step by itself',
    ).toEqual([NEEDLE]);
    expect(feed.view.history.length).toBe(before);
  });
});
