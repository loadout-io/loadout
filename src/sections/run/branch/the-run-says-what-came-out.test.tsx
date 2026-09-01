/* Bieg mówi, co po sobie zostawił — i nie obiecuje niczego, czego nie ma.
 *
 * SŁABA WERSJA tego kryterium sprawdzałaby, że przycisk istnieje. Przeszłaby dla ekranu, który
 * pozwala nacisnąć „złóż" na nazwę już zajętą — czyli dla wersji, w której człowiek dowiaduje się
 * o kolizji dopiero po tym, jak praca nie miała gdzie wylądować.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { WhatCameOut, said } from './what-came-out';

describe('the run says what came out', () => {
  it('names the branch and how many steps are on it', () => {
    expect(said({ kind: 'landed', branch: 'task-T-160', steps: 3 })).toBe(
      'The work of 3 steps is on task-T-160. Merge it when you are ready.',
    );
  });

  it('says one step without pretending there were more', () => {
    expect(said({ kind: 'landed', branch: 'feat/x', steps: 1 })).toContain('1 step is');
  });

  it('says plainly when a run changed nothing', () => {
    expect(
      said({ kind: 'nothing' }),
      'a run in which nobody wrote a byte has to say so. Silence here reads as "it worked", ' +
        'and the person goes looking for a branch that was never made.',
    ).toContain('nothing to bring together');
  });

  it('names the files two steps disagreed on, not just that they did', () => {
    const sentence = said({
      kind: 'clash',
      with: 'loadout/r1/s_2',
      files: ['src/app.ts', 'README.md'],
    });

    expect(
      sentence,
      'the clash was reported without saying where. "Two steps disagree" leaves the person with ' +
        'the question this answer was supposed to close.',
    ).toContain('src/app.ts, README.md');
    expect(
      sentence,
      'the sentence does not say that nothing was created, so a person may go looking for a ' +
        'half-made branch that deliberately does not exist.',
    ).toContain('nothing was created');
  });

  it('passes a refusal through in the words it arrived in', () => {
    expect(said('there is already a branch called task-T-160')).toBe(
      'there is already a branch called task-T-160',
    );
  });

  it('cannot be pressed before there is a name to press it with', () => {
    const drawn = renderToStaticMarkup(
      <WhatCameOut
        run="r1"
        ask={() => Promise.resolve({ name: '', convention: null, taken: false })}
        fold={() => Promise.resolve({ kind: 'nothing' as const })}
      />,
    );

    expect(
      drawn,
      'the button was live with no branch name behind it, so pressing it asks Loadout to create ' +
        'a branch called nothing.',
    ).toContain('disabled=""');
  });
});
