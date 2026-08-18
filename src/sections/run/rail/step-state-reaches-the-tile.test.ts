/* Kafelek agenta pokazuje stan, na którym NAPRAWDĘ stoi jego krok — wszystkie siedem.
 *
 * ZMIERZONA WADA (2026-08-18). Stan kroku przestawiało wyłącznie `nowRunning`, wołane dwa razy na
 * bieg: plan ze stanami `pending` przy starcie i wyzerowanie na końcu. Żadna inna droga nie
 * dotykała pola `state`, więc kafelek agenta, który właśnie edytował pliki, mówił „waiting",
 * a SZEŚĆ z siedmiu stanów [ARCHITECTURE §5] było nieosiągalnych. Rodzaj `stepState` na drucie
 * jest tym, co je dowozi.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: zawołać `roster()` z `step: 'running'` wpisanym w fakty. Przechodzi
 * dziś i przechodziłaby także wtedy, gdy nic w repo nie umie tego stanu ustawić — czyli dokładnie
 * w tej wadzie, którą ten plik zamyka. Dlatego stan powstaje TU PRZEZ KOD PRODUKCYJNY: linia
 * `stepState` wchodzi do magazynu biegu tą samą metodą, którą woła kanał (`appendLines`), a fakty
 * o agentach składamy tak, jak składa je ekran pracy (`../index.tsx`, `factsOf`) — z planu, po
 * nazwie kroku.
 *
 * PEŁNA SIÓDEMKA, nie jeden stan: wada polegała na tym, że jeden stan był osiągalny, a sześć nie.
 * Przypadek liczący tylko `running` nie odróżniłby jej od naprawy.
 */
import { describe, expect, it } from 'vitest';

import type { FeedLine, RunStore, Step, StepState } from '../../../state/run';
import { createRunStore } from '../../../state/run';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import { createFeed } from '../feed/model';
import type { AgentStatus } from './card';
import { roster } from './roster';

const BUILD = 'Build';
const STEP_ID = 's_1';

/** Ten sam plan, który wysyła kontrolka startu: na starcie wszystko czeka. */
function plan(): readonly Step[] {
  return [{ id: STEP_ID, name: BUILD, state: 'pending' }];
}

/** Wiersz `stepState` tak, jak jedzie na drucie: `state` jest zwykłym napisem. */
function stepState(id: number, state: string): FeedLine {
  return { kind: 'stepState', agent: BUILD, stepId: STEP_ID, state, id, at: id * 1_000 };
}

/**
 * Kafelki po wjechaniu tej paczki — cała droga produkcyjna, od linii do kafelka.
 *
 * Fakty o agentach składamy dokładnie tak, jak robi to ekran pracy: podpis agenta w strumieniu
 * JEST nazwą kroku (`commands/run.rs`: `forward(…, self.plan.steps[id].name.clone())`).
 */
function tilesAfter(store: RunStore, batch: readonly FeedLine[]) {
  const feed = createFeed(sealedScroller());
  feed.appendLines([line.note(1, 0, BUILD, 'Rewriting the splitter.')]);
  store.getState().appendLines(batch);
  const steps = store.getState().steps;
  return roster({
    view: feed.view,
    agents: steps.map((step) => ({
      id: step.name,
      name: step.name,
      role: '',
      step: step.state,
    })),
  });
}

/** Stan kroku → stan, którym kafelek ma o nim mówić. Ta sama siódemka, co w `ARCHITECTURE §5`. */
const EXPECTED: Readonly<Record<StepState, AgentStatus>> = {
  pending: 'waiting',
  ready: 'waiting',
  running: 'working',
  succeeded: 'done',
  failed: 'failed',
  cancelled: 'stopped',
  skipped: 'stopped',
};

describe('a step state on the wire reaches the tile of the agent running it', () => {
  it('starts from a plan whose step really is waiting, so the change below is a change', () => {
    const store = createRunStore();
    store.getState().nowRunning('Ship a feature', plan());

    expect(
      tilesAfter(store, []).at(0)?.status,
      'the tile has to say "waiting" before anything moves — otherwise "it says working now" ' +
        'would be true of a tile that always said working.',
    ).toBe('waiting');
  });

  it('moves the tile to every one of the seven states, not just the one', () => {
    for (const [state, status] of Object.entries(EXPECTED)) {
      const store = createRunStore();
      store.getState().nowRunning('Ship a feature', plan());
      const tiles = tilesAfter(store, [stepState(2, state)]);

      expect(
        tiles.at(0)?.status,
        'the wire said the step is now "' +
          state +
          '" and the tile did not follow. Six of the seven states were unreachable until the ' +
          'stepState line had a consumer: the tile of an agent editing files said "waiting", ' +
          'which is the one thing on this screen a person reads to know whether to wait.',
      ).toBe(status);
    }
  });

  it('never turns a step state into a line of history, or into a tile of its own', () => {
    const store = createRunStore();
    store.getState().nowRunning('Ship a feature', plan());
    const tiles = tilesAfter(store, [stepState(2, 'running')]);

    expect(
      tiles.length,
      'a step state is a fact about NOW, not an event to read: it moves a block on the loadout ' +
        'bar and a word on a tile. Written into history it would be four rows per step, which ' +
        'is the wall of text DESIGN §1 exists to delete.',
    ).toBe(1);
    expect(
      tiles.at(0)?.say.text,
      'and the sentence on the tile is still what the agent said, not the name of a state',
    ).toBe('Rewriting the splitter.');
  });

  it('drops a state nobody declared instead of putting it on a tile', () => {
    const store = createRunStore();
    store.getState().nowRunning('Ship a feature', plan());
    const tiles = tilesAfter(store, [stepState(2, 'constructor')]);

    expect(
      tiles.at(0)?.status,
      'a state from outside the seven reached the tile. Vendors add event types weekly and ' +
        'quietly, so an unknown value is dropped in silence — but it must not be shown, and it ' +
        'must not blank the state we already knew.',
    ).toBe('waiting');
  });
});
