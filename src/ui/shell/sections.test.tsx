/* Kryterium 3 dla T-01: widać dokładnie jedną sekcję z sześciu, a pozostałych pięciu NIE MA
 * w drzewie.
 *
 * Oczekiwana szóstka jest wypisana TUTAJ, na sztywno, a nie czytana z SECTIONS. Pętla po
 * SECTIONS sprawdzałaby rejestr sam sobą: pusta tablica przechodzi wtedy każde „dla każdej
 * sekcji…", bo nie ma żadnej.
 *
 * Rozróżnienie, o które chodzi: `expect(html).toContain('data-section="agents"')` przechodzi
 * na powłoce, która montuje wszystkie sześć sekcji i chowa pięć CSS-em. To jest dokładnie ten
 * „always-mounted route stack", przez który poprzedni prototyp renderował 142 elementy niosące tekst
 * przy suficie 60 [raport 03 §4.1]. Widać go dopiero wtedy, gdy policzy się pozostałe pięć
 * identyfikatory DO ZERA i zabroni `hidden` oraz `display:none`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { SECTIONS } from '../sections';

/** Kolejność i podpisy są z ARCHITECTURE.md §3 i decyzji D5. Jedno słowo, tryb rozkazujący. */
const EXPECTED = [
  { id: 'run', label: 'Run' },
  { id: 'workflows', label: 'Workflows' },
  { id: 'agents', label: 'Agents' },
  { id: 'skills', label: 'Skills' },
  { id: 'memory', label: 'Memory' },
  { id: 'triggers', label: 'Triggers' },
] as const;

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

function markupFor(id: (typeof EXPECTED)[number]['id']): string {
  return renderToStaticMarkup(<App section={id} />);
}

describe('one section of six is on screen and the other five are not in the tree', () => {
  it('registers the six, in order, under the words a person reads', () => {
    expect(
      SECTIONS.length,
      'SECTIONS has to hold exactly six entries — the six top-level places this app has. It ' +
        'holds ' +
        String(SECTIONS.length),
    ).toBe(EXPECTED.length);
    expect(
      SECTIONS.map((entry) => entry.id),
      'the order is part of the contract: it is the order of the switcher and of ' +
        'ARCHITECTURE.md §3',
    ).toEqual(EXPECTED.map((entry) => entry.id));
    expect(
      SECTIONS.map((entry) => entry.label),
      'the words on screen are English and one word each (decision D5)',
    ).toEqual(EXPECTED.map((entry) => entry.label));
  });

  for (const entry of EXPECTED) {
    it('mounts ' + entry.id + ' once and leaves the other five out of the tree', () => {
      const markup = markupFor(entry.id);
      expect(
        occurrences(markup, 'data-section="' + entry.id + '"'),
        'asking for ' +
          entry.id +
          ' has to put exactly one element carrying data-section="' +
          entry.id +
          '" in the tree',
      ).toBe(1);
      for (const other of EXPECTED) {
        if (other.id === entry.id) continue;
        expect(
          occurrences(markup, 'data-section="' + other.id + '"'),
          'with ' +
            entry.id +
            ' open, ' +
            other.id +
            ' has to be absent from the tree, not merely invisible. Six mounted and five hidden ' +
            'is the shape that put 142 text-carrying elements on one poprzedni prototyp screen',
        ).toBe(0);
      }
    });

    it('never hides a section instead of leaving it out, with ' + entry.id + ' open', () => {
      const markup = markupFor(entry.id);
      expect(
        / hidden(?:=""|>|\s)/.test(markup),
        'nothing in the shell may carry the hidden attribute: hiding is how five sections stay ' +
          'mounted while the measurement above still passes',
      ).toBe(false);
      expect(
        /display\s*:\s*none/i.test(markup),
        'nothing in the shell may set display:none, for the same reason. Which section is open ' +
          'is decided in TypeScript, not in a style sheet',
      ).toBe(false);
    });

    it('says which section is open exactly once, with ' + entry.id + ' open', () => {
      const markup = markupFor(entry.id);
      expect(
        occurrences(markup, 'aria-current="true"'),
        'one fact, one place (invariant 13): exactly one element says which section is open. ' +
          'poprzedni prototyp showed the connection state in six places at once',
      ).toBe(1);
      expect(
        occurrences(markup, 'aria-current="page"'),
        'the value is true, not page. There are no pages here and no addresses — no router at ' +
          'all (T8 §6.2)',
      ).toBe(0);
    });
  }
});
