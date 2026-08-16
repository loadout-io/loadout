/* Kryterium 4 dla T-26: sekcja Memory montuje się naprawdę i trzyma dwie strefy osobno.
 *
 * Powód dwóch połów i kontroli przeciw pustej asercji jest wypisany raz, w
 * `src/sections/workflows/mounted.test.tsx`. Tutaj drugą połową jest ROZDZIAŁ STREF i to on
 * jest całym produktem tej sekcji: notatka zaproponowana nie wchodzi do promptu, dopóki
 * człowiek jej nie promuje (T-17), więc ekran wyświetlający obie w jednym worku kasuje jedyną
 * widoczną różnicę między tym, co zaproponował agent, a tym, co zatwierdził człowiek. „Obie
 * notatki są w dokumencie" przechodzi na jednej płaskiej liście — czyli na ekranie, który tę
 * sekcję unieważnia. Dlatego pytamy o strefy, a nie o obecność.
 *
 * KONTRAKT NA MARKUP. Każda strefa niesie `data-zone` — `suggested` albo `in-use`. Kawałek
 * markupu strefy to wszystko od jej znacznika do znacznika następnej strefy, więc jedna płaska
 * lista daje jedną strefę i wywraca porównania niżej niezależnie od kolejności stref.
 *
 * CZEGO TO KRYTERIUM NIE MIERZY I DLACZEGO — ZGŁOSZENIE DLA CZŁOWIEKA (zmierzone 2026-08-16).
 * `tasks/T-26.md` chce, żeby notatka zaproponowana niosła „swoje DWIE akcje" (makieta:
 * `Use it` i `Discard`, `docs/mockup/index.html:757`). `NoteRow` renderuje dokładnie JEDNĄ —
 * `Use this` przy `suggested`, `Stop using` przy `in-use` — i tak zamraża to kryterium 6
 * z T-17. Drugiej nie ma czym obsłużyć: `MemoryState` zna `use`, `stopUsing` i `cancel`,
 * i ani jednego odrzucenia kandydatki, a przycisk bez handlera nie wchodzi do repo
 * (niezmiennik 16) — to jest dokładnie ta wada, którą T-26 cytuje jako powód swojego
 * istnienia. Asercja niżej wymaga więc JEDNEJ akcji, tej, która istnieje, i nie udaje, że
 * mierzy dwie. Domknięcie wymaga `discard` w `src/state/memory.ts` i drugiego przycisku
 * w `src/sections/memory/note-row.tsx` — oba pliki są poza blokiem OWNS tego zadania
 * (AGENTS.md §7).
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from '../../App';
import type { Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { sectionEntry } from '../../ui/sections';
import MemoryScreen from './index';

/** Zdanie pustego ekranu PAMIĘCI — nie zdanie pustej sekcji z rejestru. */
const NO_NOTES_YET = 'No notes yet.';

/** Kandydatka: agent ją zaproponował, człowiek jeszcze nie powiedział „tak". */
const WAITING: Note = {
  id: 'n-1',
  title: 'Quote handling needs a state machine',
  rule: 'Prefer small state machines over regex',
  because: 'Character-by-character checks miss embedded separators.',
  status: 'suggested',
  scope: 'this-project',
  length: 137,
  occurrences: 3,
  modified: '2026-08-16T09:00:00Z',
};

/** Notatka w użyciu: wchodzi do promptu każdego agenta w tym projekcie. */
const IN_USE: Note = {
  id: 'n-2',
  title: 'Locks and waiting',
  rule: 'Never hold a lock across an await',
  because: 'One held lock and one slow read is the whole deadlock.',
  status: 'in-use',
  scope: 'this-project',
  length: 96,
  occurrences: 8,
  modified: '2026-08-14T11:30:00Z',
};

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Kawałek markupu od znacznika tej strefy do znacznika następnej. */
function zone(markup: string, id: string): string {
  const start = markup.indexOf('data-zone="' + id + '"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-zone="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

/** Treść pierwszego elementu z `data-state` w tym kawałku — chip stanu z `NoteRow`. */
function chipIn(part: string): string {
  const hit = /<([a-z]+)[^>]*\bdata-state\b[^>]*>([\s\S]*?)<\/\1>/i.exec(part);
  return (hit?.[2] ?? '')
    .replace(/<[^>]*>/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

beforeEach(() => {
  /* Magazyn notatek jest singletonem, więc zasianie go w jednym teście dojechałoby do
   * następnego. Stan pusty przed każdym: kolejność testów przestaje mieć znaczenie. */
  useMemory.setState({ notes: [], message: null, choice: null });
});

describe('the memory section mounts for real and keeps the two zones apart', () => {
  it('mounts through real discovery and says its own sentence when there is nothing', () => {
    const markup = renderToStaticMarkup(<App section="memory" />);

    expect(
      markup,
      'asking the shell for memory WITHOUT handing it screens has to reach the file on disk. ' +
        'The note row has been landed and green since T-17 and was mounted by nobody',
    ).toContain(NO_NOTES_YET);
    expect(
      markup,
      'the section has its own empty sentence now, so the one the registry keeps for memory ' +
        'has no business being in the document as well (invariant 13)',
    ).not.toContain(sectionEntry('memory').empty);
  });

  it('control: with no screen in hand the shell still says the registry sentence', () => {
    const markup = renderToStaticMarkup(<App section="memory" screens={{}} />);

    expect(
      markup,
      'the control against an empty assertion: without it, "the registry sentence is gone" ' +
        'also passes on a shell that stopped rendering that sentence at all',
    ).toContain(sectionEntry('memory').empty);
  });

  it('keeps what waits for a person out of the zone that goes into every prompt', () => {
    useMemory.setState({ notes: [WAITING, IN_USE] });

    const markup = renderToStaticMarkup(<MemoryScreen store={useMemory} />);
    const waiting = zone(markup, 'suggested');
    const inUse = zone(markup, 'in-use');

    expect(
      waiting,
      'the note an agent suggested belongs in the zone that waits for a person. One flat list ' +
        'passes "both notes are in the document" and erases the only visible difference ' +
        'between what an agent proposed and what a person approved',
    ).toContain(WAITING.rule);
    expect(
      waiting,
      'and the note that is already in use may not be in that zone as well — two zones that ' +
        'both hold everything are one list with two headings',
    ).not.toContain(IN_USE.rule);
    expect(inUse, 'the note in use belongs in the zone that goes into every prompt').toContain(
      IN_USE.rule,
    );
    expect(inUse, 'and the one still waiting may not be there').not.toContain(WAITING.rule);

    expect(
      chipIn(waiting),
      'the waiting note carries its own marker, the one the landed row draws. A screen that ' +
        'lays out its own row and drops the chip looks right and says nothing',
    ).toBe('Suggested');
    expect(chipIn(inUse), 'and the note in use carries the other one').toBe('In use');
    expect(
      occurrences(waiting, 'data-act'),
      'the waiting note carries the action a person came here to take. Read the note at the ' +
        'head of this file before changing this number: the mockup draws two actions and only ' +
        'one of them has anything behind it today',
    ).toBe(1);
  });
});
