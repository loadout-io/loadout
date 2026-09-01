/* Małpka otwiera listę miejsc tylko tam, gdzie naprawdę wskazuje miejsce.
 *
 * SŁABA WERSJA tego kryterium pytałaby, czy tekst zawiera `@`. Przeszłaby dla implementacji, która
 * otwiera listę plików w środku adresu e-mail — czyli dla wady, nie dla naprawy.
 */
import { describe, expect, it } from 'vitest';

import { chosen, mentionAt } from './at-mention';

describe('the at sign opens a place picker only where it means a place', () => {
  it('opens at the start of an empty message', () => {
    expect(mentionAt('@', 1), 'the first character typed did not count').toEqual({
      at: 0,
      typed: '',
    });
  });

  it('carries what was typed after it as the query', () => {
    expect(mentionAt('put it in @src/sec', 18)?.typed).toBe('src/sec');
  });

  it('stays shut inside an email address', () => {
    expect(
      mentionAt('write to jakub@konghq.com', 20),
      'an address opened the file picker. A person writing an email address is not pointing at ' +
        'a folder, and a list that appears there covers the sentence they are writing.',
    ).toBeNull();
  });

  it('closes once a space says the pointing is over', () => {
    expect(
      mentionAt('@src and then', 13),
      'the picker stayed open after the person moved on to the sentence, so it keeps offering ' +
        'paths for a query that is no longer a path.',
    ).toBeNull();
  });

  it('is shut when the caret sits before any at sign', () => {
    expect(mentionAt('word @src', 4), 'a mention was found behind the caret').toBeNull();
  });

  it('leaves the caret inside a folder, ready to go deeper', () => {
    const mention = mentionAt('in @sr', 6);
    expect(mention).not.toBeNull();
    const put = chosen('in @sr', mention!, 'src/');
    expect(put.text, 'the chosen folder did not replace the mention').toBe('in src/');
    expect(
      put.caret,
      'the caret did not land inside the folder, so going one level deeper costs a click ' +
        'instead of a keystroke.',
    ).toBe('in src/'.length);
  });

  it('closes a file with a space, so the next word is not glued to its name', () => {
    const mention = mentionAt('@READ', 5);
    const put = chosen('@READ', mention!, 'README.md');
    expect(put.text).toBe('README.md ');
    expect(put.caret).toBe('README.md '.length);
  });
});
