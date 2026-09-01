/* Kryterium 2 dla T-25: sekcja bez ekranu pokazuje zdanie ze swojego wpisu w rejestrze,
 * a nie puste miejsce.
 *
 * Dwie słabe wersje tej asercji i powód, dla którego obie są nic niewarte:
 *   `expect(html.length).toBeGreaterThan(0)` przechodzi na pustym `<main>`, czyli na dokładnie
 *   tym białym prostokącie, przed którym to kryterium ma bronić.
 *   Porównanie z ZDANIEM WKLEJONYM W TEST przestaje cokolwiek znaczyć w dniu, w którym ktoś
 *   poprawi brzmienie w `src/ui/sections.tsx` — a rozjazd między rejestrem a ekranem jest
 *   właśnie tym, co miało tu zostać złapane (niezmiennik 13: jeden fakt, jedno miejsce).
 * Dlatego oczekiwana wartość jest CZYTANA z rejestru przez `sectionEntry(id).empty`.
 *
 * Identyfikatory są za to wypisane na sztywno: gdyby pętla brała je z SECTIONS, pusty rejestr
 * przeszedłby każde „dla każdej sekcji…", nie sprawdzając ani jednej (ta sama pułapka, co
 * w sections.test.tsx z T-01).
 *
 * Ostatnia asercja — siedem RÓŻNYCH zdań — jest osobno, bo bez niej cały plik przechodzi na
 * powłoce, która wpisuje jedno zdanie wszędzie i akurat trafiła w rejestr jednym z siedmiu.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { App } from '../../App';
import { sectionEntry } from '../sections';

const EXPECTED = [
  'run',
  'workflows',
  'agents',
  'skills',
  'memory',
  'triggers',
  'settings',
] as const;

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Treść jedynego elementu z `data-empty`, bez znaczników i bez nadmiarowych odstępów. */
function emptyStateText(markup: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-empty\b[^>]*>([\s\S]*?)<\/\1>/i.exec(markup);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function markupFor(id: (typeof EXPECTED)[number]): string {
  return renderToStaticMarkup(<App section={id} screens={{}} />);
}

describe('a section with no screen shows its own sentence, not a blank space', () => {
  it('greets memory with exactly the sentence its entry holds', () => {
    const markup = markupFor('memory');
    expect(
      occurrences(markup, 'data-empty'),
      'memory has no screen here, so the shell has to render exactly one empty screen for it',
    ).toBe(1);
    expect(
      emptyStateText(markup),
      'the words on an empty screen come from sectionEntry(id).empty and from nowhere else. ' +
        'A second copy of the sentence inside the shell drifts away from the entry the first ' +
        'time somebody rewords it, and nothing would say so',
    ).toBe(sectionEntry('memory').empty);
  });

  for (const id of EXPECTED) {
    it('greets an empty ' + id + ' with the sentence its entry holds', () => {
      const markup = markupFor(id);
      expect(
        occurrences(markup, 'data-empty'),
        id + ' has to render exactly one element carrying data-empty',
      ).toBe(1);
      expect(
        emptyStateText(markup),
        'with no screen for ' +
          id +
          ', the shell has to say what its entry says. The entry reads ' +
          JSON.stringify(sectionEntry(id).empty),
      ).toBe(sectionEntry(id).empty);
    });
  }

  it('gives the seven sections seven sentences of their own', () => {
    const said = EXPECTED.map((id) => emptyStateText(markupFor(id)));
    expect(
      new Set(said).size,
      'seven sections, seven sentences. One sentence reused everywhere passes every comparison ' +
        'above for whichever section it was copied from, and reads like a bug on the other ' +
        'six; the shell said: ' +
        JSON.stringify(said),
    ).toBe(EXPECTED.length);
  });
});
