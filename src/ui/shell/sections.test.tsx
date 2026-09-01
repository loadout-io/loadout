/* Kryterium 3 dla T-01: widać dokładnie jedną sekcję z ośmiu, a pozostałych siedmiu NIE MA
 * w drzewie.
 *
 * Oczekiwana ósemka jest wypisana TUTAJ, na sztywno, a nie czytana z SECTIONS. Pętla po
 * SECTIONS sprawdzałaby rejestr sam sobą: pusta tablica przechodzi wtedy każde „dla każdej
 * sekcji…", bo nie ma żadnej.
 *
 * Rozróżnienie, o które chodzi: `expect(html).toContain('data-section="agents"')` przechodzi
 * na powłoce, która montuje wszystkie sekcje i chowa wszystkie poza jedną CSS-em. To jest
 * dokładnie ten „always-mounted route stack", przez który poprzedni prototyp renderował 142 elementy
 * niosące tekst przy suficie 60 [raport 03 §4.1]. Widać go dopiero wtedy, gdy policzy się
 * pozostałe identyfikatory DO ZERA i zabroni `hidden` oraz `display:none`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { SECTIONS } from '../sections';

/**
 * Kolejność i podpisy. Jedno słowo, tryb rozkazujący (decyzja D5).
 *
 * KOLEJNOŚĆ ZMIENIONA 2026-08-31 i to jest zmiana produktu, nie porządków w pliku. Stało tu
 * `run, workflows, agents, …`, czyli od końca drogi do jej początku: człowiek, który otwiera
 * aplikację pierwszy raz, czytał jako pierwszą pozycję jedyną rzecz, której zrobić nie może —
 * bez agenta nie ma workflow, a bez workflow nie ma czego uruchomić.
 *
 * WYROCZNIĄ TEJ KOLEJNOŚCI JEST MAKIETA, nie ta tablica: `shell-matches-mockup.test.tsx` czyta
 * `<nav class="nav">` z `docs/mockup/index.html` w tym samym biegu i porównuje etykiety wiersz
 * po wierszu. Ten punkt pilnuje czego innego — że rejestr niesie SIEDEM pozycji, te i tylko te,
 * i że nikt nie zgubił żadnej po drodze. Tablica jest wypisana NA SZTYWNO z premedytacją: pętla
 * po `SECTIONS` sprawdzałaby rejestr samym sobą, a scalenie gubiące sekcję przeszłoby bez śladu.
 *
 * (Wiersz `knowledge` jest jeden zamiast dwóch od 2026-08-31: Skills i Memory zeszły się, bo obie
 * odpowiadały na jedno pytanie człowieka — „co ten model wie o mojej pracy" — i kazały mu
 * wybierać dwa razy.)
 */
const EXPECTED = [
  { id: 'agents', label: 'Agents' },
  { id: 'workflows', label: 'Workflows' },
  { id: 'run', label: 'Run' },
  { id: 'triggers', label: 'Triggers' },
  { id: 'knowledge', label: 'Knowledge' },
  { id: 'lab', label: 'Lab' },
  { id: 'settings', label: 'Settings' },
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
            ' has to be absent from the tree, not merely invisible. Every section mounted and ' +
            'all but one hidden is the shape that put 142 text-carrying elements on one ' +
            'the earlier prototype screen',
        ).toBe(0);
      }
    });

    it('never hides a section instead of leaving it out, with ' + entry.id + ' open', () => {
      const markup = markupFor(entry.id);
      expect(
        / hidden(?:=""|>|\s)/.test(markup),
        'nothing in the shell may carry the hidden attribute: hiding is how the other sections ' +
          'stay mounted while the measurement above still passes',
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
          'the earlier prototype showed the connection state in six places at once',
      ).toBe(1);
      expect(
        occurrences(markup, 'aria-current="page"'),
        'the value is true, not page. There are no pages here and no addresses — no router at ' +
          'all (T8 §6.2)',
      ).toBe(0);
    });
  }
});
