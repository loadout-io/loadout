/* Kryterium 5: lista kafelków bierze się ze strumienia, nie z planu [T2 §9.2].
 *
 * `expect(roster(state).length).toBe(3)` przechodzi dla implementacji, która bierze listę
 * agentów z definicji workflow i odfiltrowuje te, które jeszcze nie ruszyły. Liczba się
 * zgadza, kafelek wygląda dobrze, a kafelka pod-agenta nie ma i nigdy nie będzie — bo
 * pod-agenta nie ma w żadnym workflow i nie da się go tam wpisać z góry.
 *
 * Rozróżniają to dwie asercje w PRZECIWNYCH kierunkach:
 *   nie ma  — kafelka agenta kroku, który został pominięty, mimo że plan go zna,
 *   jest    — kafelek agenta, którego plan nie zna wcale.
 * Obie naraz przechodzą tylko wtedy, gdy źródłem jest strumień. Trzecia asercja domyka to
 * od strony kolejności: plan wymienia agentów inaczej, niż nadali, więc lista ułożona
 * po planie przechodzi obie pierwsze i wykłada się tutaj.
 *
 * Scena jest ułożona tak, żeby te trzy rzeczy dały się rozróżnić naraz:
 *   krok 4  pominięty; Rivet nie nadał ani jednej linii,
 *   krok 2  Forge rozpuszcza Scouta, którego nie ma w planie,
 *   krok 3  anulowany PO tym, jak Needle coś nadał — kafelek zostaje, stan brzmi `stopped`.
 */
import { describe, expect, it } from 'vitest';
import { line } from '../feed/fixtures/lines';
import { sealedScroller } from '../feed/fixtures/scroller';
import { createFeed } from '../feed/model';
import type { AgentFacts, RosterInput } from './roster';
import { roster } from './roster';

/**
 * Plan: czterech agentów na cztery kroki, w kolejności grafu.
 *
 * Kolejność tej listy jest CELOWO inna niż kolejność nadawania w strumieniu niżej. Lista
 * kafelków ułożona po planie jest nie do odróżnienia od poprawnej dopóty, dopóki oba
 * porządki się zgadzają — a w prawdziwym biegu równoległym nie zgadzają się prawie nigdy.
 */
const PLAN: readonly AgentFacts[] = [
  { id: 'Orion', name: 'Orion', role: 'lead', step: 'running' },
  { id: 'Forge', name: 'Forge', role: 'writes code', step: 'running' },
  { id: 'Needle', name: 'Needle', role: 'runs checks', step: 'cancelled' },
  { id: 'Rivet', name: 'Rivet', role: 'second opinion', step: 'skipped' },
];

function fourStepRun(): RosterInput {
  const feed = createFeed(sealedScroller());
  feed.appendLines([
    line.note(1, 0, 'Orion', 'Four steps; the last one only runs if the checks fail.'),
    line.read(2, 100, 'Needle', 'tests/parser.rs'),
    line.edit(3, 200, 'Forge', 'src/parser.rs', 42, 8),
    // Scout jest w strumieniu i nie ma go w planie. Tak wygląda każdy pod-agent.
    line.read(4, 300, 'Scout', 'docs/csv-edge-cases.md'),
    line.handoff(5, 400, 'Forge', 'Forge → Needle'),
  ]);
  return { view: feed.view, agents: PLAN };
}

describe('the list of agents is built from what happened, not from what was planned', () => {
  it('lists exactly the agents that said something, in the order they first did', () => {
    const cards = roster(fourStepRun());

    expect(
      cards.map((card) => card.id),
      'first appearance in the run, not position in the plan. The plan names them Orion, ' +
        'Forge, Needle, Rivet; the run heard Orion, Needle, Forge, Scout',
    ).toEqual(['Orion', 'Needle', 'Forge', 'Scout']);
  });

  it('has no place for the agent of a step that was skipped', () => {
    const ids = roster(fourStepRun()).map((card) => card.id);

    expect(
      ids.includes('Rivet'),
      'Rivet is in the plan and never said a word. A list drawn from the plan shows it ' +
        'anyway, "so you can see what is coming" — and draws a relation the data does not ' +
        'have (invariant 17)',
    ).toBe(false);
  });

  it('has a place for an agent the plan never heard of', () => {
    const ids = roster(fourStepRun()).map((card) => card.id);

    expect(
      ids.includes('Scout'),
      'Scout was started by Forge in the middle of the run. No workflow can name it in ' +
        'advance, so a list drawn from the plan can never show it — and this is the half of ' +
        'the rule that a count alone will never catch',
    ).toBe(true);
  });

  it('keeps the agent whose step was called off, and says so in one word', () => {
    const needle = roster(fourStepRun()).find((card) => card.id === 'Needle');

    expect(needle, 'Needle read a file before the step was called off, so it stays').toBeDefined();
    expect(
      needle?.status,
      'and its state is "stopped". Dropping the card would erase work that really happened; ' +
        'leaving it as "working" would show an agent that is not there',
    ).toBe('stopped');
  });
});
