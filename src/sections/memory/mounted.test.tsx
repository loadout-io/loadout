/* Kryterium 4 dla T-26: półka notatek montuje się naprawdę i trzyma dwie strefy osobno.
 * (Do 2026-08-31 była własną sekcją; dziś stoi w sekcji Knowledge obok półki umiejętności.)
 *
 * Powód dwóch połów i kontroli przeciw pustej asercji jest wypisany raz, w
 * `src/sections/workflows/mounted.test.tsx`. Tutaj drugą połową jest ROZDZIAŁ STREF i to on
 * jest całym produktem tej sekcji: notatka zaproponowana nie wchodzi do promptu, dopóki
 * człowiek jej nie promuje (T-17), więc ekran wyświetlający obie w jednym worku kasuje jedyną
 * widoczną różnicę między tym, co zaproponował agent, a tym, co zatwierdził człowiek. „Obie
 * notatki są w dokumencie" przechodzi na jednej płaskiej liście — czyli na ekranie, który tę
 * sekcję unieważnia. Dlatego pytamy o strefy, a nie o obecność.
 *
 * KONTRAKT NA MARKUP. Każda strefa niesie `data-zone` — `suggested`, `in-use` albo `passed`. Kawałek
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
import type { Handoff, Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { useSkills } from '../../state/skills';
import { sectionEntry } from '../../ui/sections';
import KnowledgeScreen from '../knowledge';
import NotesShelf from './shelf';

/** Zdanie pustego ekranu PAMIĘCI — nie zdanie pustej sekcji z rejestru. */
/* Zaproszenie ekranu Knowledge — od 2026-08-31 ono, a nie „No notes yet.": obie półki dzielą
 * jedno zdanie o pustce, bo dwa byłyby dwiema odpowiedziami na jedno pytanie (niezmiennik 13). */
const NOTHING_HERE_YET = 'Nothing here yet.';

/**
 * Magazyn umiejętności, który już odpowiedział i nie ma nic do powiedzenia.
 *
 * Podmiotem tego pliku są notatki. Bez tego ekran liczyłby „przeczytane" z magazynu, którego
 * nikt nie zasiał, i mówiłby „jeszcze czytam" niezależnie od tego, co odpowiedziały notatki.
 */
function quietSkills(): typeof useSkills {
  useSkills.setState({ folders: 'read', installed: [], pending: null, message: null });
  return useSkills;
}

/** Kandydatka: agent ją zaproponował, człowiek jeszcze nie powiedział „tak". */
const WAITING: Note = {
  place: 'project',
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
  place: 'project',
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

/** Plik, który jeden agent zostawił drugiemu — trzecia strefa ekranu. */
const PASSED: Handoff = {
  id: 'h-1',
  run: '0198a1f2-3b4c-7d5e-8f60-99887766aabb',
  from: 'Scout',
  to: ['Forge'],
  kind: 'findings',
  title: 'What the quote parser actually does',
  status: 'current',
  created: '2026-08-16T09:02:11Z',
  path: '/Users/someone/work/.loadout/runs/2026-08-16__abc/handoffs/02__scout__findings.md',
  bytes: 3174,
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
   * następnego. Stan pusty przed każdym: kolejność testów przestaje mieć znaczenie.
   * WSZYSTKIE pola, także `passed` i `passedProblem`: pole pominięte tutaj przecieka między
   * testami i pierwszym objawem jest test, który przechodzi tylko w swojej kolejności.
   *
   * `read: true` DOPISANE 2026-08-31, i nie jest to rozluźnienie. Sekcja ma od tego dnia trzy
   * odpowiedzi zamiast dwóch: „jeszcze czytam", „nic tu nie ma", „nie dało się przeczytać".
   * Podmiotem tego pliku jest DRUGA z nich, więc fikstura mówi to, co mówił jej brak w świecie
   * o dwóch odpowiedziach: dyski są przeczytane i nic w nich nie było. Bez tego pola kryterium
   * pytałoby o zdanie pustego ekranu na ekranie, który jeszcze nie skończył czytać. */
  useMemory.setState({
    notes: [],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
    read: true,
  });
});

describe('the notes shelf mounts for real and keeps its zones apart', () => {
  /* PYTANIE ZOSTAŁO, SELEKTOR SIĘ ZMIENIŁ. Do 2026-08-31 notatki miały własną sekcję i ten
   * przypadek pytał o `<App section="memory" />`. Sekcja nazywa się dziś Knowledge i trzyma
   * obie półki; pytanie jest to samo — czy prawdziwe odkrywanie ekranów dowozi TĘ zawartość
   * do okna, czy tylko zdanie z rejestru. */
  it('mounts through real discovery, with the notes on screen and no registry sentence', () => {
    useMemory.setState({ notes: [IN_USE] });
    const markup = renderToStaticMarkup(<App section="knowledge" />);

    expect(
      markup,
      'asking the shell for knowledge WITHOUT handing it screens has to reach the file on disk. ' +
        'The note row has been landed and green since T-17 and was mounted by nobody',
    ).toContain(IN_USE.rule);
    expect(
      markup,
      'the section has a screen of its own now, so the sentence the registry keeps for a ' +
        'section with NO screen has no business being in the document (invariant 13)',
    ).not.toContain(sectionEntry('knowledge').empty);
  });

  it('control: with no screen in hand the shell still says the registry sentence', () => {
    const markup = renderToStaticMarkup(<App section="knowledge" screens={{}} />);

    expect(
      markup,
      'the control against an empty assertion: without it, "the registry sentence is gone" ' +
        'also passes on a shell that stopped rendering that sentence at all',
    ).toContain(sectionEntry('knowledge').empty);
  });

  it('keeps what waits for a person out of the zone that goes into every prompt', () => {
    useMemory.setState({ notes: [WAITING, IN_USE] });

    const markup = renderToStaticMarkup(<NotesShelf store={useMemory} />);
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

/* Trzecia strefa: „What agents passed to each other".
 *
 * DLACZEGO TO JEST OSOBNY BLOK, A NIE ASERCJA W POPRZEDNIM. Poprzedni blok pilnuje ROZDZIAŁU
 * dwóch połów jednej listy notatek. Ten pilnuje czegoś innego: obietnicy, którą rejestr sekcji
 * składa na jej pustym ekranie („What agents leave for each other lands here",
 * `src/ui/sections.tsx`) i której do 2026-08-18 nie realizował żaden ekran — sekcja rysowała
 * DWIE strefy z trzech i nie miała ani jednej drogi, którą mogłaby zapytać o te pliki.
 *
 * JAK BRZMIAŁABY SŁABA WERSJA I CO JĄ ODRÓŻNI. Słaba wersja to „nagłówek trzeciej strefy jest
 * w dokumencie". Przechodzi na strefie, która rysuje nagłówek i nigdy nie pokazuje ani jednego
 * pliku — czyli na dokładnie tym samym braku, tylko z podpisem. Odróżniają ją dwie rzeczy:
 * nazwa pliku Z FIKSTURY w środku strefy i asercja, że pusta strefa mówi ZDANIE, a nie milczy.
 */
describe('the third zone shows the files agents leave for each other', () => {
  it('is on screen with nothing in it, and says why instead of showing a heading over air', () => {
    useMemory.setState({ notes: [IN_USE], passed: [] });

    const markup = renderToStaticMarkup(<NotesShelf store={useMemory} />);
    const passed = zone(markup, 'passed');

    expect(
      passed,
      'the registry promises this on the empty screen of the section, so the zone that keeps ' +
        'that promise may not be the one zone the screen leaves out',
    ).not.toBe('');
    expect(
      passed,
      'and an empty zone says why it is empty. These files appear when a step finishes and ' +
        'hands its result on, and nothing has finished on this machine yet — a heading with ' +
        'nothing under it leaves a person guessing whether the section is broken',
    ).toContain('Nothing yet.');
  });

  it('shows the file, who left it for whom, and how big it is', () => {
    useMemory.setState({ notes: [], passed: [PASSED] });

    /* EKRAN, NIE SAMA PÓŁKA, i tylko w tym jednym przypadku. Pytanie brzmi „czy sekcja pełna
       przekazań udaje pustą", a to zdanie stoi na ekranie Knowledge — półka go nie zna i pytanie
       zadane półce przechodziłoby na niczym. Reszta pliku pyta półkę, bo pyta o strefy. */
    const markup = renderToStaticMarkup(
      <KnowledgeScreen notes={useMemory} skills={quietSkills()} />,
    );
    const passed = zone(markup, 'passed');

    expect(
      markup,
      'a section holding files an agent left is NOT empty, whatever the note list says. The ' +
        'empty-screen sentence in its place would hide the only thing on screen',
    ).not.toContain(NOTHING_HERE_YET);
    expect(
      passed,
      'the file is named by the name it has in the folder — that is what makes "open them ' +
        'anywhere" true rather than friendly',
    ).toContain('02__scout__findings.md');
    expect(passed, 'and by its full address, so it can be found').toContain(PASSED.path);
    expect(
      passed,
      'who left it and for whom. This comes from the file, not from the order of the list',
    ).toContain('Scout → Forge');
    expect(
      passed,
      'and its size, read from the wire and written the way the mockup writes it',
    ).toContain('3.1 KB');

    expect(
      passed.includes('<a '),
      'the mockup draws each row as a link, and this wave has no command that opens a file, so ' +
        'a link here would be a control that answers a click with nothing — worse than none ' +
        '(invariant 16). The address on screen is what replaces it',
    ).toBe(false);
  });

  it('keeps a refusal to read those files inside that zone', () => {
    useMemory.setState({
      notes: [IN_USE],
      passed: [],
      passedProblem: 'Loadout could not read what agents passed to each other.',
    });

    const markup = renderToStaticMarkup(<NotesShelf store={useMemory} />);

    expect(
      zone(markup, 'passed'),
      'notes and these files live in two different folders, so one sentence for both would ' +
        'leave a person guessing which one to go and look at',
    ).toContain('could not read what agents passed');
  });
});
