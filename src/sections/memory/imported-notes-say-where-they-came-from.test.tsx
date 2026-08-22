/* Kryterium 5 dla T-80: wiersz Pamięci mówi, skąd notatka przyszła i czyja jest.
 *
 * `Note` ma od commitu kontraktowego dwa pola — `agent` (czyja to wiedza) i `from` (z jakiego
 * projektu przyjechała) — i `note-row.tsx` nie czyta ani jednego. Import ma je wypełnić
 * (`MemoryNote::agent`, `MemoryNote::source`), a na ekranie nie ma ich gdzie zobaczyć: notatka
 * przywieziona z cudzego repozytorium wygląda dokładnie tak samo jak zdanie, które ktoś napisał
 * tutaj ręcznie. To jest ten sam kształt, co reszta szwów w tym repo — pole istnieje, drut je
 * niesie, odbiorcy nie ma.
 *
 * SŁABĄ WERSJĄ TEGO KRYTERIUM JEST JEDEN RENDER I `toContain`. Przechodzi na wierszu, który
 * wypisuje właściciela ZAWSZE — czyli na tym, który notatce niczyjej dopisuje myślnik, słowo
 * „unassigned" albo ostatnią nazwę, jaką widział. Człowiek czyta taki wiersz jako fakt o
 * notatce, a jest to fakt o komponencie. Rozróżniają to dwa warianty z TEGO SAMEGO komponentu:
 * notatka o zakresie jednego agenta i notatka, która nie należy do nikogo.
 *
 * DRUGĄ SŁABĄ WERSJĄ JEST TEST BEZ KOTWICY. Wiersz, który nie renderuje nic, spełnia każde
 * `not.toContain` na świecie — dlatego pierwsze zdanie każdego wariantu pyta o regułę, czyli
 * o to jedno, co ten wiersz miał pokazywać od T-17.
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom` ani
 * `@testing-library/react` — `package.json` nie należy do tego zadania.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Note } from '../../state/memory';
import { NoteRow } from './note-row';

/** Nazwa agenta tak, jak zapisał ją człowiek w pliku notatki. */
const OWNER = 'backend-dev';

/** Projekt, z którego ta notatka przyjechała przy imporcie. */
const CAME_FROM = 'acme-checkout';

const OWNED_RULE = 'Drain the queue in one place, or two runs put the same job through twice.';
const NOBODYS_RULE = 'An unresolved tenant comes back as 401, not 400.';
const REASON = 'run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88';

/** Wyróżniająca się liczba: przypadkowe trafienie w klasę albo w rok byłoby fałszywą zielenią. */
const LENGTH = 137;
const MODIFIED = '2026-08-20T10:31:02Z';

/* Każde z tych słów stoi tu jako DANE, nie jako copy: pojedyncze słowo bez spacji nie jest
 * prozą dla sprawdzacza słownictwa, więc lista nie musi być nigdzie wyjęta ani wyciszona.
 * Są to nazwy, którymi wiersz UDAJE odpowiedź na pytanie, na które odpowiedzi nie ma. */
const NEVER_INVENTED = ['unassigned', 'unknown', 'nobody', 'everyone', 'anonymous'];

/** Notatka jednego agenta, przywieziona z cudzego projektu. */
function ownedNote(): Note {
  return {
    id: 'the-queue-is-drained-in-one-place',
    title: 'The queue is drained in one place',
    rule: OWNED_RULE,
    because: REASON,
    status: 'in-use',
    scope: 'this-agent',
    length: LENGTH,
    occurrences: 1,
    modified: MODIFIED,
    agent: OWNER,
    from: CAME_FROM,
  };
}

/** Notatka, która nie należy do nikogo i nie ma udawać, że należy. */
function nobodysNote(): Note {
  return {
    id: 'the-tenant-is-resolved-before-the-guard',
    title: 'The tenant is resolved before the guard',
    rule: NOBODYS_RULE,
    because: REASON,
    status: 'in-use',
    scope: 'this-project',
    length: LENGTH,
    occurrences: 1,
    modified: MODIFIED,
  };
}

function noop(): void {
  /* sterowany wiersz: w statycznym renderze nic tego nie woła */
}

function markup(one: Note): string {
  return renderToStaticMarkup(<NoteRow note={one} onUse={noop} onStopUse={noop} />);
}

describe('a note row says whose knowledge it is and which project it came from', () => {
  it('names the agent a note reaches, when the note reaches only one agent', () => {
    const html = markup(ownedNote());

    expect(
      html,
      'the row shows the sentence the note carries, first. Without this line everything below ' +
        'is also true of a row that renders nothing at all, which is the one failure a list of ' +
        'expected words cannot see on its own',
    ).toContain(OWNED_RULE);

    expect(
      html,
      'a note that reaches one agent has to say WHICH agent, on the row. The scope word alone ' +
        'says a name exists somewhere and not what it is, so a person deciding whether to keep ' +
        'this sentence cannot tell whose work it will change',
    ).toContain(OWNER);
  });

  it('says which project an imported note was carried over from', () => {
    const html = markup(ownedNote());

    expect(html, 'the same row, still showing what the note says').toContain(OWNED_RULE);
    expect(
      html,
      'and it says where this came from. The same sentence can be carried over from two ' +
        'projects, and a row that never shows the origin reads the second copy as a second ' +
        'fact — after which nobody can tell which of the two they are about to retire',
    ).toContain(CAME_FROM);
  });

  it('leaves a note that belongs to nobody without an owner, and without a stand-in for one', () => {
    const html = markup(nobodysNote());

    expect(html, 'the row shows the sentence this note carries').toContain(NOBODYS_RULE);
    expect(
      html,
      'this note belongs to nobody, and a row that shows an owner anyway is answering from the ' +
        'component instead of from the note. A row that prints the last name it saw passes the ' +
        'two checks above and is wrong about every note in the list but one',
    ).not.toContain(OWNER);
    expect(html, 'and it was written here, so there is no other project to point at').not.toContain(
      CAME_FROM,
    );

    const lower = html.toLowerCase();
    for (const word of NEVER_INVENTED) {
      expect(
        lower,
        'this word reached the screen in place of an owner. A dash or a filler word answers a ' +
          'question nobody asked, and a person reads the answer as a fact about the note ' +
          '(invariant 13: one live region per fact, and none at all for a fact there is not). ' +
          'The word was: ' +
          word,
      ).not.toContain(word);
    }
  });
});
