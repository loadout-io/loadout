/* Listy bez sufitu mówią, ILE ich jest, zanim pokażą siebie.
 *
 * # Co było zmierzone
 *
 * `skills-row.tsx` i `borrow-row.tsx` renderowały po jednym polu wyboru na KAŻDĄ pozycję,
 * zawsze, bez licznika. Repozytorium z trzydziestoma umiejętnościami zamieniało panel kroku
 * w ścianę pól wyboru w kolumnie 330 px — i to bez żadnej granicy, bo obie listy przychodzą
 * z cudzego katalogu, więc ich długość nie jest niczym ograniczona.
 *
 * # Czego to kryterium pilnuje
 *
 * Że zwinięta lista NAZYWA LICZBĘ: ile jest do wzięcia i ile już wzięto. Sam licznik bez listy
 * byłby ślepym zaułkiem, a sama lista bez licznika jest tym, co było. Liczby są liczone z tego,
 * co wiersz naprawdę dostał — dwie długości list, żeby napis wpisany na stałe nie przeszedł
 * (niezmiennik 20).
 *
 * Pola wyboru muszą przy tym ZOSTAĆ. Wiersz, który zwinął listę przez jej skasowanie, przechodzi
 * każde sprawdzenie mówiące „nie ma tu trzydziestu pól" i zabiera jedyną drogę do wybrania
 * czegokolwiek.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import type { HostMaterial } from '../../../state/workflows';
import { BorrowRow } from './borrow-row';
import { SkillsRow } from './skills-row';

function noop(): void {
  /* sterowane wiersze: statyczny render nic z tego nie woła */
}

function manySkills(count: number): string[] {
  return Array.from({ length: count }, (_, index) => 'skill-' + String(index + 1));
}

function material(count: number): HostMaterial {
  return {
    skills: manySkills(count),
    learnings: ['backend-dev'],
    subagents: ['release-engineer'],
  };
}

/** Napis zwiniętej listy, czyli to jedno zdanie, które zostaje z całej listy. */
function saysWhenShut(html: string): string {
  const hit = /<summary\b[^>]*>([\s\S]*?)<\/summary>/.exec(html);
  return (hit?.[1] ?? '').replace(/<[^>]*>/g, '').trim();
}

/** Ile pól wyboru wiersz naprawdę wystawia. */
function boxes(html: string): number {
  return [...html.matchAll(/<input\b[^>]*type="checkbox"[^>]*>/g)].length;
}

describe('a list with no ceiling says how long it is before it unrolls', () => {
  it('counts the skills on offer and the ones already picked', () => {
    const thirty = renderToStaticMarkup(
      <SkillsRow
        mode="subset"
        runsWith="claude-code"
        available={manySkills(30)}
        value={['skill-1', 'skill-2']}
        onChoose={noop}
      />,
    );

    expect(
      saysWhenShut(thirty),
      'thirty tick boxes unroll into the step panel with nothing saying how many there are. In ' +
        'a 330 px column that is a wall, and the person scrolls past the rest of the panel to ' +
        'get out of it',
    ).toBe('30 to choose from, 2 picked');
    expect(
      boxes(thirty),
      'the list was shortened by throwing it away, so there is no longer any way to pick one',
    ).toBe(30);

    expect(
      saysWhenShut(
        renderToStaticMarkup(
          <SkillsRow
            mode="subset"
            runsWith="claude-code"
            available={manySkills(4)}
            value="all"
            onChoose={noop}
          />,
        ),
      ),
      'the count stands where it was written rather than following the list. Two lists, ' +
        'because one passes for a sentence typed under the row',
    ).toBe('4 to choose from');
  });

  it('counts what this project lends and what the step already takes', () => {
    const html = renderToStaticMarkup(
      <BorrowRow
        material={material(12)}
        value={{ skills: ['skill-1'], learnings: 'backend-dev' }}
        onChoose={noop}
      />,
    );

    expect(
      saysWhenShut(html),
      'everything this project lends unrolls into the panel at once, and nothing says how much ' +
        'of it there is or how much this step already takes',
    ).toBe('14 to borrow, 2 taken');
    expect(
      boxes(html),
      'the shelves were folded away by deleting them, so nothing can be borrowed any more',
    ).toBe(14);
    expect(
      saysWhenShut(
        renderToStaticMarkup(<BorrowRow material={material(3)} value={{}} onChoose={noop} />),
      ),
      'and with nothing taken yet the row says only what is on offer',
    ).toBe('5 to borrow');
  });
});
