/* Dwa kafelki „uruchom i zostaw" biorą DWIE różne komendy.
 *
 * # Zamówienie
 *
 * Właściciel, 2026-08-30: „agent sam ma rozkminić jakie komendy użyć do odpalenia… nie chcę
 * w każdym projekcie osobno wpisywać na front i backend command".
 *
 * Liczba mnoga jest tu treścią, nie stylem. Pierwsza wersja tego mechanizmu miała nazwę pola
 * zaszytą na `command`, więc kafelek frontu i kafelek backendu przeczytałyby TO SAMO — czyli
 * uruchomiłyby dwa razy jedną rzecz i nie powiedziałyby o tym ani słowa.
 *
 * # Czego to kryterium pilnuje najmocniej
 *
 * Ostatniego przypadku: nazwa liczona jest RAZ i zapisana. Przeliczanie jej przy każdym renderze
 * znaczyłoby, że przemianowanie kafelka po okablowaniu po cichu rozłącza graf — krok przed nim
 * oddaje dalej starą nazwę, a bieg dowiaduje się o tym dopiero w środku.
 */
import { describe, expect, it } from 'vitest';

import type { AgentStep, ServeStep, WorkflowFile } from '../../../state/workflows';
import {
  DESCRIBE_THE_COMMAND,
  askTheStepBefore,
  fieldNameFor,
  handsOver,
  theStepBefore,
} from './hands-over-the-command';

function agent(id: string, name: string): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: 'a1',
    overrides: {},
    copies: 1,
    instructions: 'work it out',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 0, y: 0 },
  };
}

function serve(id: string, name: string): ServeStep {
  return { kind: 'serve', id, name, command: '', folder: { use: 'same-copy' }, at: { x: 0, y: 0 } };
}

/** Jeden agent, dwa serwery za nim — kształt z zamówienia. */
function frontAndBack(): WorkflowFile {
  return {
    format: 1,
    id: 'w1',
    name: 'Preview',
    steps: [
      agent('s_dev', 'Build it'),
      serve('s_front', 'Run frontend'),
      serve('s_back', 'Run backend'),
    ],
    links: [
      { from: 's_dev', to: 's_front' },
      { from: 's_dev', to: 's_back' },
    ],
  };
}

describe('the command a serve tile waits for', () => {
  it('gives two tiles two different field names, with nothing to type', () => {
    const doc = frontAndBack();
    const front = doc.steps[1] as ServeStep;
    const back = doc.steps[2] as ServeStep;

    expect(fieldNameFor(front)).toBe('run-frontend');
    expect(fieldNameFor(back)).toBe('run-backend');
    expect(
      fieldNameFor(front) === fieldNameFor(back),
      'one fixed name would hand both tiles the same command — two starts of one thing, and not ' +
        'a word about it anywhere',
    ).toBe(false);
  });

  it('falls back to the tile id when the name gives nothing to type', () => {
    expect(fieldNameFor({ id: 's_app', name: '!!!' })).toBe('s-app');
  });

  it('finds the agent step the arrow comes from', () => {
    expect(theStepBefore(frontAndBack(), 's_front')?.id).toBe('s_dev');
    expect(
      theStepBefore(frontAndBack(), 's_dev'),
      'a tile nothing points at has nobody to hand it a command',
    ).toBe(null);
  });

  it('asks the step before, and asking twice changes nothing', () => {
    const once = askTheStepBefore(frontAndBack(), 's_front', 'run-frontend');
    const twice = askTheStepBefore(once, 's_front', 'run-frontend');

    const before = once.steps.find((step) => step.id === 's_dev') as AgentStep;
    expect(before.handover).not.toBe('notes');
    expect(handsOver(before, 'run-frontend')).toBe(true);
    expect(
      (before.handover as { fields: unknown[] }).fields[0],
      'the description is what the agent reads. Without it the field name is a riddle it has to ' +
        'guess',
    ).toEqual({ name: 'run-frontend', describe: DESCRIBE_THE_COMMAND, required: true });
    expect(twice).toEqual(once);
  });

  it('adds to what the step already hands over, never replaces it', () => {
    const doc = askTheStepBefore(frontAndBack(), 's_front', 'run-frontend');
    const both = askTheStepBefore(doc, 's_back', 'run-backend');
    const before = both.steps.find((step) => step.id === 's_dev') as AgentStep;

    expect(
      (before.handover as { fields: { name: string }[] }).fields.map((one) => one.name),
      'one agent hands over both commands. Replacing on the second ask would silently unwire the ' +
        'first tile — and the run would only say so once it got there',
    ).toEqual(['run-frontend', 'run-backend']);
  });

  it('marks the field as needed, because without it the tile refuses', () => {
    const doc = askTheStepBefore(frontAndBack(), 's_front', 'run-frontend');
    const before = doc.steps.find((step) => step.id === 's_dev') as AgentStep;
    const asked = (before.handover as { fields: { required?: boolean }[] }).fields[0];

    expect(
      asked?.required,
      'a tile with no command refuses to start. Asking for it as optional would be a request ' +
        'whose refusal costs the whole run up to that point',
    ).toBe(true);
  });
});
