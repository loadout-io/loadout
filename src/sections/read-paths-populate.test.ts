/* AC-6 dla T-38: Pamięć i Umiejętności pokazują to, co leży na dysku, a nie to, co dodano w tej
 * sesji.
 *
 * ZMIERZONA WADA, KTÓRĄ TO KRYTERIUM ZAMYKA. Do 2026-08-18 nie istniała ani `list_notes`, ani
 * `list_skills`. `install_skill` zapisywało umiejętność na dysk, okno nigdy tego nie odczytywało,
 * więc licznik „N saved" pokazywał wyłącznie to, co dodano w TEJ sesji — a po restarcie
 * zainstalowana umiejętność znikała z ekranu, leżąc dalej w katalogach agentów. To jest
 * niezmiennik 4 złamany wprost: pliki są prawdą, a ekran mówił co innego.
 *
 * SŁABA WERSJA I CO JĄ ODRÓŻNIA. Słaba wersja tego kryterium woła krawędź sekcji wprost —
 * `io.<odczyt>()` — i sprawdza, co wróciło. Odpowiada wtedy na pytanie „czy funkcja działa",
 * a pytanie brzmi „czy sekcja PYTA": dziś nie pyta i właśnie dlatego zapisane umiejętności giną
 * po restarcie. Odróżniają je trzy rzeczy, wszystkie niżej:
 *
 *   1. wołana jest wyłącznie ŚCIEŻKA WEJŚCIA magazynu, a sądzony jest STAN magazynu — nie to,
 *      co oddała funkcja;
 *   2. `@tauri-apps/api/core` jest podmieniony atrapą, więc cała droga magazyn → krawędź sekcji
 *      → `invoke` jedzie kodem produkcyjnym, a atrapa stoi dopiero na granicy okna. To repo nie
 *      ma okna Tauri, a kryterium go wymagające nie umie być czerwone z właściwego powodu
 *      (`Failed to launch` jest na liście `NOT_A_REAL_RED`);
 *   3. ten plik NIE MA PRAWA nazwać funkcji odczytu z krawędzi sekcji — pilnuje tego asercja
 *      na jego własnym źródle. Bez niej pierwsza wygodna poprawka zamienia to kryterium
 *      z powrotem w jego słabą wersję i nikt tego nie zauważy.
 *
 * WARTOŚCI OCZEKIWANE CZYTANE Z PLIKU. Nazwy komend nie są tu wpisane z palca: wybiera je ze
 * `src-tauri/commands.golden.txt` reguła niżej, a osobna asercja żąda, żeby wybrała DOKŁADNIE
 * jedną — porównanie dwóch pustych wartości przechodzi na niczym. Złota lista jest jedynym
 * miejscem, w którym obie strony szwu zgadzają się co do nazwy (`ipc_commands_registered.rs`
 * trzyma po stronie Rusta, że ta lista równa się `generate_handler!`).
 *
 * Pliki czytamy przez `existsSync(p) ? readFileSync(p) : ''`, żeby test padał na asercji
 * o treści, nigdy na otwarciu pliku (AGENTS.md §2a p. 5).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Note } from '../state/memory';
import { useMemory } from '../state/memory';
import type { InstalledSkill } from '../state/skills';
import { useSkills } from '../state/skills';

/* Atrapa podniesiona razem z `vi.mock`, żeby moduły sekcji dostały JĄ, a nie prawdziwy
 * transport. Zachowanie ustawia każdy przypadek osobno — ten sam plik sądzi i odpowiedź,
 * i odmowę. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..', '..');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Jedyna lista nazw komend. Ten sam plik czyta po drugiej stronie `ipc_commands_registered.rs`. */
const GOLDEN: readonly string[] = fileText(resolve(ROOT, 'src-tauri/commands.golden.txt'))
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line !== '' && !line.startsWith('#'));

/**
 * Nazwa komendy odczytu dla tej sekcji, WYBRANA ZE ZŁOTEJ LISTY.
 *
 * Wpisana z palca przeszłaby także wtedy, gdy `ipc.rs` mówi co innego — czyli sprawdzałaby samą
 * siebie. To jest dokładnie ta wada, przez którą powstało całe T-38: `start-invokes.test.tsx`
 * przepisał dwie nazwy argumentów z trzech i był zielony przy Starcie odrzucanym za każdym
 * kliknięciem.
 */
function readCommandFor(thing: string): string {
  const found = GOLDEN.filter((name) => name.startsWith('list_') && name.includes(thing));
  expect(
    found,
    'src-tauri/commands.golden.txt has to name exactly one read command for ' +
      thing +
      ', and it names ' +
      String(found.length) +
      ' (' +
      found.join(', ') +
      '). Zero is the state this task exists to end: the section can write to disk and has no ' +
      'way to read back, so it shows what was added since the window opened, not what is on disk.',
  ).toHaveLength(1);
  return found.at(0) ?? '';
}

const NOTES_COMMAND = readCommandFor('note');
const SKILLS_COMMAND = readCommandFor('skill');

/* Notatki i umiejętności „z dysku". Kształt jest lustrem tego, co oddaje Rust — pola sądzi po
 * tamtej stronie `ipc_read_paths.rs`, porównując je z tymi samymi plikami `src/state/*.ts`. */
function note(id: string, title: string): Note {
  return {
    id,
    title,
    rule: 'OKAPI-' + id + ' the files are the truth; the screen shows the files.',
    because: 'measured 2026-08-18 on a window that never read the disk',
    status: 'in-use',
    scope: 'this-project',
    length: 24,
    occurrences: 1,
    modified: '2026-08-18T09:15:00Z',
  };
}

const ON_DISK_NOTES: readonly Note[] = [
  note('tenant-before-guard', 'The tenant is resolved before the guard'),
  note('index-is-disposable', 'The index is disposable'),
];

const ON_DISK_SKILLS: readonly InstalledSkill[] = [
  { name: 'pdf', fromTheInternet: true },
  { name: 'release-notes', fromTheInternet: false },
];

/** Świeży magazyn, ten sam kształt, w którym okno startuje. */
const BLANK_MEMORY = useMemory.getState();
const BLANK_SKILLS = useSkills.getState();

/** Atrapa odpowiada TĄ listą wyłącznie na TĘ komendę; na każdą inną oddaje pustą. */
function answers(command: string, payload: readonly unknown[]): void {
  invoked.mockImplementation((...sent: unknown[]) =>
    Promise.resolve(sent.at(0) === command ? [...payload] : []),
  );
}

/** Atrapa odmawia każdej komendzie — tak wygląda Rust, który nie umie przeczytać katalogu. */
function refuses(refusal: unknown): void {
  invoked.mockImplementation(() => Promise.reject(refusal));
}

/** Nazwy komend, które naprawdę pojechały do Rusta, w kolejności wywołania. */
function commandsSent(): string[] {
  return invoked.mock.calls.map((call) => String(call.at(0)));
}

beforeEach(() => {
  invoked.mockReset();
  invoked.mockImplementation(() => Promise.resolve([]));
  useMemory.setState(BLANK_MEMORY, true);
  useSkills.setState(BLANK_SKILLS, true);
});

describe('entering Memory and Skills reads the disk instead of remembering what was added', () => {
  it('(a) a fresh Memory store carries the notes the read command handed back', async () => {
    answers(NOTES_COMMAND, ON_DISK_NOTES);
    expect(
      useMemory.getState().notes,
      'the store has to start empty, otherwise this case cannot tell a read path from a seed',
    ).toEqual([]);

    await useMemory.getState().load();

    /* 2026-08-18 — ASERCJA ROZSZERZONA, NIE OSLABIONA, i to jest cala jej tresc.
     *
     * Do tego dnia stalo tu `toEqual([NOTES_COMMAND])`, czyli „wejscie w Pamiec pyta o DOKLADNIE
     * jedna rzecz". To bylo prawda przez przypadek: sekcja renderowala dwie strefy z trzech,
     * a trzecia — „What agents passed to each other", naglowna obietnica tej sekcji wedlug
     * zdania z rejestru — nie miala ZADNEJ drogi odczytu. Kiedy droga powstala (`list_handoffs`),
     * ta asercja zapalila sie na PRAWIDLOWEJ zmianie.
     *
     * Rozstrzygniecie nie moze byc `toContain`: to zdjeloby wlasnosc „raz", ktora jest tu
     * najcenniejsza — sekcja pytajaca o `list_notes` przy kazdym renderze wyglada dokladnie tak
     * samo jak sekcja poprawna, tylko pali IPC. Wiec pytamy o dwie rzeczy osobno: ze `list_notes`
     * padlo DOKLADNIE RAZ, i ze zadna komenda nie padla dwa razy. Odczyt trzeciej strefy wolno
     * dolozyc; drugie zapytanie o TO SAMO nie. */
    const sent = commandsSent();
    expect(
      sent.filter((name) => name === NOTES_COMMAND),
      'entering the Memory section has to ask Rust once, by the name on ' +
        'src-tauri/commands.golden.txt. A section that never asks shows a note a person ' +
        'approved yesterday as if it were not there.',
    ).toEqual([NOTES_COMMAND]);
    expect(
      sent.filter((name, at) => sent.indexOf(name) !== at),
      'no command may be asked twice for one entry into the section. Two reads of the same ' +
        'thing are two answers to one question (invariant 13), and the second one is the one ' +
        'that goes stale.',
    ).toEqual([]);
    expect(
      useMemory.getState().notes,
      'the notes on disk did not reach the store. The section then renders "No notes yet." over ' +
        'a directory full of notes, which is invariant 4 broken where a person can see it.',
    ).toEqual(ON_DISK_NOTES);
  });

  it('(b) a fresh Skills store carries the skills the read command handed back', async () => {
    answers(SKILLS_COMMAND, ON_DISK_SKILLS);
    expect(
      useSkills.getState().installed,
      'the store has to start empty, otherwise this case cannot tell a read path from a seed',
    ).toEqual([]);

    await useSkills.getState().load();

    expect(
      commandsSent(),
      'entering the Skills section has to ask Rust once, by the name on ' +
        'src-tauri/commands.golden.txt. Without the question, "N saved" counts what was added since ' +
        'added and a skill installed last week is invisible until it is installed again.',
    ).toEqual([SKILLS_COMMAND]);
    expect(
      useSkills.getState().installed,
      'the skills on disk did not reach the store, so the screen disagrees with the agent ' +
        'directories it is supposed to describe',
    ).toEqual(ON_DISK_SKILLS);
    expect(
      useSkills.getState().installed.map((one) => one.fromTheInternet),
      'the marker that stands in for the signatures v1 does not have has to survive the read. ' +
        'Lost on the way back, every skill pulled off a link looks like one a person wrote.',
    ).toEqual([true, false]);
  });

  it('(c) entering a section twice does not show the same thing twice', async () => {
    answers(NOTES_COMMAND, ON_DISK_NOTES);
    await useMemory.getState().load();
    await useMemory.getState().load();

    expect(
      useMemory.getState().notes,
      'the second entry into the section appended the same notes again instead of replacing ' +
        'them. A person then sees every note twice and the counter above the section counts the ' +
        'same files twice.',
    ).toEqual(ON_DISK_NOTES);

    answers(SKILLS_COMMAND, ON_DISK_SKILLS);
    await useSkills.getState().load();
    await useSkills.getState().load();

    expect(
      useSkills.getState().installed.map((one) => one.name),
      'the second entry into Skills appended the same skills again. Two rows for one directory ' +
        'is a list a person stops trusting, and "N saved" stops being a count of anything.',
    ).toEqual(ON_DISK_SKILLS.map((one) => one.name));
  });

  it('(d) a refusal leaves an honest empty section and never throws upward', async () => {
    const SAID = 'Loadout has no permission to read that folder.';
    refuses(new Error(SAID));

    /* Gdyby ścieżka wejścia przepuszczała wyjątek, ten `await` by go rzucił i przypadek padłby
     * TUTAJ, przed pierwszą asercją. Wejście w sekcję jest wołane z `useEffect`, gdzie
     * odrzuconej obietnicy nie ma kto złapać — a nieobsłużone odrzucenie wywraca ekran zamiast
     * pokazać zdanie. */
    await useMemory.getState().load();
    await useSkills.getState().load();

    expect(
      useMemory.getState().notes,
      'a refused read has to leave the section honestly empty. Notes kept from before the ' +
        'refusal are what the section REMEMBERS, not what is in the files.',
    ).toEqual([]);
    expect(
      useSkills.getState().installed,
      'a refused read has to leave the section honestly empty, for the same reason',
    ).toEqual([]);
    expect(
      [useMemory.getState().message, useSkills.getState().message],
      "Rust's own sentence has to reach the screen. A refusal in silence looks exactly like a " +
        'broken button, and a person who is not told what is wrong clicks again and files a bug.',
    ).toEqual([SAID, SAID]);
  });

  it('(d) a refusal that carries no sentence still says something, in plain English', async () => {
    /* Odmowa, która nie jest `Error`: przez granicę IPC jedzie zwykła wartość, więc to jest
     * kształt, w którym Rust naprawdę potrafi odmówić. */
    refuses({ code: 42 });

    await useMemory.getState().load();
    await useSkills.getState().load();

    for (const said of [useMemory.getState().message, useSkills.getState().message]) {
      expect(
        said ?? '',
        'a refusal with no sentence of its own still has to leave one, or the section is empty ' +
          'and silent and nobody can tell an empty folder from an unreadable one',
      ).not.toEqual('');
      for (const jargon of [NOTES_COMMAND, SKILLS_COMMAND, 'invoke', 'IPC', 'undefined']) {
        expect(
          said ?? '',
          'the sentence a person reads must not carry a name from the wire (invariant 14): ' +
            jargon,
        ).not.toContain(jargon);
      }
    }
  });

  it('judges the section, not the edge: this file never calls the read function itself', () => {
    const source = fileText(resolve(HERE, 'read-paths-populate.test.ts'));
    expect(
      source.length,
      'this case reads its own source and found nothing, so it would pass over anything',
    ).toBeGreaterThan(2000);

    /* Nazwy sklejone, żeby ta asercja nie znalazła samej siebie. Ten sam chwyt co w AC-2. */
    for (const forbidden of ['list' + 'Notes', 'list' + 'Skills']) {
      expect(
        source.includes(forbidden),
        'this file names the read function from sections/*/io.ts. Calling it here would prove ' +
          'that the function works and say nothing about whether the section ASKS — and not ' +
          'asking is the whole defect: today the screen shows what was added, not the disk.',
      ).toBe(false);
    }
  });
});
