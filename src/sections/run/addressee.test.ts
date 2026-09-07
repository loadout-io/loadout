/* Zdanie bez ukośnika idzie do lidera; do agenta wyłącznie po nazwie.
 *
 * Zgłoszenie właściciela 2026-08-20: „proza w trakcie biegu znika z rozmowy z liderem, bo leci
 * do pracującego agenta". Do dziś rozstrzygał jeden warunek w `./index.tsx` — ktoś pracuje,
 * więc zdanie idzie do niego — a to znaczy, że lider milczał dokładnie w tych minutach, w których
 * człowiek chce zapytać, co się dzieje.
 *
 * SŁABA WERSJA: test wyłącznie na pustej liście pracujących. Przechodzi dla DZISIEJSZEJ
 * implementacji, która przy jednym pracującym wysyła do agenta — czyli dla wady, którą to
 * kryterium zamyka. Rozróżnia to przypadek z jednym pracującym, i dlatego stoi tuż pod tamtym.
 */
import { describe, expect, it } from 'vitest';

import { addresseeOf, sessionAddresseeOf } from './addressee';
import type { StepSession } from '../../ipc/types';

/** Zdanie, które człowiek pisze w środku biegu, kiedy chce wiedzieć, co się dzieje. */
const ASKING = 'what is left before this is done';

describe('prose goes to the lead agent unless it starts with the name of a working step', () => {
  it('goes to the lead when nobody is working', () => {
    expect(
      addresseeOf(ASKING, []),
      'with nothing running there is nobody else to talk to, and the conversation about what ' +
        'should happen next is the whole point of the lead agent being reachable at all',
    ).toEqual({ to: 'lead', text: ASKING });
  });

  it('still goes to the lead when exactly one step is working', () => {
    expect(
      addresseeOf(ASKING, ['Forge']),
      'this is the change. Until now one working step captured every sentence, so the lead agent ' +
        'went silent for the length of the run — exactly when a person wants to ask what is ' +
        'happening, and exactly when sending that question to somebody who is writing code is ' +
        'both useless and paid for.',
    ).toEqual({ to: 'lead', text: ASKING });
  });

  it('reaches a working step by name, and takes the name out of what it will hear', () => {
    expect(
      addresseeOf('Forge use tabs instead of spaces', ['Forge']),
      'naming a working step at the start of the line is the one way to reach it — the same ' +
        'convention Rust already answers with when several are working. And the name has to come ' +
        'OFF the sentence: a step told "Forge use tabs" is being addressed by its own name, ' +
        'which reads like somebody quoting it back at itself.',
    ).toEqual({ to: 'agent', agent: 'Forge', text: 'use tabs instead of spaces' });
  });

  it('does not treat the name of a step that is not working as an address', () => {
    expect(
      addresseeOf('Forge use tabs instead of spaces', ['Needle']),
      'a step that is not working cannot hear anything, so its name is an ordinary word. The ' +
        'sentence goes to the lead agent WHOLE, with that word still in it: dropping the first ' +
        'word on the way would change the sentence a person wrote, and nothing on the screen ' +
        'would say so.',
    ).toEqual({ to: 'lead', text: 'Forge use tabs instead of spaces' });
  });

  it('matches whole words, so a shorter name is not an address for a longer one', () => {
    expect(
      addresseeOf('Plan the work before touching anything', ['Planner']),
      'the match is on the whole word. On a prefix, "Plan the work" would be delivered to ' +
        'Planner with its first word missing — the wrong reader AND a sentence that no longer ' +
        'says what it said.',
    ).toEqual({ to: 'lead', text: 'Plan the work before touching anything' });
  });
});

/* Kanał, który bieg naprawdę otworzył — jedyny fakt, z którego wolno budować adres. */
const RUN = '01980000-0000-7000-8000-000000000001';
const channel = (agent: string, nodeKey: string, finished = false): StepSession => ({
  runId: RUN,
  nodeKey,
  agent,
  canReceive: true,
  finished,
});

describe('an address is a whole name somebody really opened, not the first word of the line', () => {
  it('leaves a tile that never opened a channel as an ordinary word', () => {
    expect(
      sessionAddresseeOf('Backend is still broken, what happened?', []),
      'the plan is not a list of addresses. A tile that never opened a channel cannot hear ' +
        'anything, so its name is an ordinary word and the question goes to the lead agent ' +
        'WHOLE. Feeding the plan in here was worst after a run: the tiles of the last run stay ' +
        'on screen while nothing is running, and every question that happened to start with one ' +
        'of their names came back refused — while the row under the field promised the lead.',
    ).toEqual({ to: 'lead', text: 'Backend is still broken, what happened?' });
  });

  it('reaches one copy of a step by the multi-word name the row prints', () => {
    expect(
      sessionAddresseeOf('Builder (2 of 3) use tabs instead of spaces', [
        channel('Builder (1 of 3)', 'builder'),
        channel('Builder (2 of 3)', 'builder~2'),
      ]),
      'a step with copies is registered as "Builder (2 of 3)" and that is exactly the name the ' +
        'row under the field tells a person to start the line with. Reading only the first word ' +
        'left "Builder", which nobody answers to, so the one address the screen advertises was ' +
        'the one address that did not work.',
    ).toEqual({
      to: 'agent',
      target: channel('Builder (2 of 3)', 'builder~2'),
      text: 'use tabs instead of spaces',
    });
  });

  it('keeps a recipient that already stopped addressable, so the run answers instead of silence', () => {
    expect(
      sessionAddresseeOf('Builder look at this again', [channel('Builder', 'builder', true)]),
      'the name still belongs to somebody, so the sentence has to go there and come back with ' +
        'the honest answer that the recipient has finished. Dropping it to the lead agent ' +
        'instead would send it somewhere the person did not address, without saying so.',
    ).toEqual({
      to: 'agent',
      target: channel('Builder', 'builder', true),
      text: 'look at this again',
    });
  });

  it('does not read a longer word as a name that is a piece of it', () => {
    expect(
      sessionAddresseeOf('Builders never agree on tabs', [channel('Builder', 'builder')]),
      'the match ends on a word boundary. Without it the sentence would be delivered with a ' +
        'piece of its first word cut off — the wrong reader AND a sentence that no longer says ' +
        'what it said.',
    ).toEqual({ to: 'lead', text: 'Builders never agree on tabs' });
  });
});
