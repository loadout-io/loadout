/* Przycisk „Add loop": pętla robiona WSKAZANIEM DWÓCH KAFELKÓW.
 *
 * DLACZEGO TO ISTNIEJE, a nie wystarczy pociągnięcie strzałki. Powrót idzie z dolnej kropki
 * sędziego na GÓRNĄ kropkę kroku, do którego wraca praca — czyli w bok i do góry, obok innych
 * kafelków. Kto minie tę kropkę o kilka pikseli i puści nad korpusem, dostawał krok-widmo
 * (`onConnectEnd`), a po jego skasowaniu uwagę o strzałce celującej w krok, którego nie ma.
 * Zgłoszenie właściciela, 2026-08-22.
 *
 * ODMOWA JEST TU ZDANIEM, NIE CISZĄ, i to jest różnica wobec `isValidConnection`. Tam człowiek
 * ciągnie strzałkę i widzi, że nie łapie; tutaj klika dwa kafelki i bez zdania nie wiedziałby,
 * czy pętla powstała. Dlatego każde kryterium niżej sądzi TREŚĆ zdania, a nie samo `null`.
 *
 * SŁABĄ WERSJĄ jest sprawdzenie „dokument się zmienił". Przechodzi ją implementacja, która dokłada
 * strzałkę BEZ sufitu tur — a taki plik walidator Rusta odrzuca, więc gest kończyłby się
 * workflow, którego nie da się zapisać.
 */
import { describe, expect, it } from 'vitest';
import type { AgentStep, WorkflowFile } from '../../../state/workflows';
import { TURNS_BY_DEFAULT, addLoop } from './connect';

function step(id: string, name: string, y: number): AgentStep {
  return {
    kind: 'agent',
    id,
    name,
    agent: '019897b4-8f3a-7c21-9d44-0b6a1e2c5f70',
    overrides: {},
    copies: 1,
    instructions: 'Do the part of the work this tile owns.',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at: { x: 24, y },
  };
}

/** Kształt z ekranu właściciela: plan, dwie gałęzie, każda ze swoim sprawdzeniem. */
function file(): WorkflowFile {
  return {
    format: 1,
    id: 'wf_ship_a_feature',
    name: 'Ship a feature',
    steps: [
      step('plan', 'Final implementation plan', 24),
      step('front', 'Front', 168),
      step('design', 'Figma check', 312),
      step('back', 'Backend implementation', 168),
      step('checked', 'Backend check', 312),
      step('ship', 'Ship it', 456),
    ],
    links: [
      { from: 'plan', to: 'front' },
      { from: 'front', to: 'design' },
      { from: 'plan', to: 'back' },
      { from: 'back', to: 'checked' },
      { from: 'design', to: 'ship' },
      { from: 'checked', to: 'ship' },
    ],
  };
}

describe('picking two tiles sends the work back from the check to the step that has to redo it', () => {
  it('adds the way back WITH a limit, and says nothing is wrong', () => {
    const made = addLoop('checked', 'back', file());

    expect(made.refused, 'the pair is fine, so there is nothing to tell the person').toBeNull();
    expect(
      made.file.links,
      'a way back without a limit is a file the validator refuses, so the gesture would end in a ' +
        'workflow that cannot be saved. The number lands in the same move as the arrow',
    ).toEqual([...file().links, { from: 'checked', to: 'back', max_turns: TURNS_BY_DEFAULT }]);
  });

  it('lifts the arrow that is already there instead of drawing a second one', () => {
    const straight: WorkflowFile = {
      ...file(),
      links: [...file().links, { from: 'checked', to: 'back' }],
    };

    const made = addLoop('checked', 'back', straight);

    expect(made.refused, 'raising an arrow that is already drawn is not a refusal').toBeNull();
    expect(
      made.file.links.filter((link) => link.from === 'checked' && link.to === 'back'),
      'two arrows between one pair of steps share a single identity on the canvas, so one of ' +
        'them is simply not drawn while the file carries both',
    ).toEqual([{ from: 'checked', to: 'back', max_turns: TURNS_BY_DEFAULT }]);
  });

  it('takes a SECOND loop when the two branches share no step', () => {
    const front = addLoop('design', 'front', file());
    const both = addLoop('checked', 'back', front.file);

    expect(both.refused, 'two branches with a check each is an ordinary working day').toBeNull();
    expect(
      both.file.links.filter((link) => link.max_turns !== undefined),
      'and both really land, each carrying its own limit',
    ).toEqual([
      { from: 'design', to: 'front', max_turns: TURNS_BY_DEFAULT },
      { from: 'checked', to: 'back', max_turns: TURNS_BY_DEFAULT },
    ]);
  });

  it('turns down a loop that would cross one already drawn, and says which two', () => {
    const front = addLoop('design', 'front', file());
    const crossing = addLoop('ship', 'plan', front.file);

    expect(
      crossing.refused,
      'this one would repeat everything, the front branch included, so it swallows the loop ' +
        'that is already there. Neither one could then say which round leaves for the work ' +
        'after it, and Rust refuses that file',
    ).toContain('cross another one');
    expect(crossing.file, 'and a refused pair leaves the document untouched').toBe(front.file);
  });

  it('turns down a step that does not run after the other one, and names both', () => {
    const wrongWay = addLoop('front', 'checked', file());

    expect(
      wrongWay.refused,
      'nothing leads from the backend check to the front step, so there is no work to send ' +
        'back — the person clicked two tiles that are not on one path',
    ).toContain('does not run after');
    expect(
      wrongWay.refused,
      'and the sentence names the two tiles by the names on them, so the person does not have ' +
        'to work out which pair was wrong',
    ).toContain('"Front"');
    expect(wrongWay.refused).toContain('"Backend check"');
  });

  it('turns down the same step twice and a check that already sends the work back', () => {
    expect(
      addLoop('front', 'front', file()).refused,
      'a step cannot wait for itself: there is no body to repeat',
    ).toContain('two different steps');

    const once = addLoop('checked', 'back', file());
    expect(
      addLoop('checked', 'plan', once.file).refused,
      'one check writes one outcome, so a second way back out of it would be two answers to ' +
        'one question',
    ).toContain('already sends the work back');
  });
});
