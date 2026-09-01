/* „Nic tu nie ma" kłamie przy starcie: sekcja ma trzy odpowiedzi, nie dwie.
 *
 * ZMIERZONA WADA (2026-08-31). Ekran znał dwa stany świata — „są notatki" i „nie ma notatek" —
 * a stanów jest trzy: JESZCZE NIE WIEM, nie ma, nie dało się przeczytać. Wejście w sekcję
 * odpala odczyt w efekcie, czyli PO pierwszym malowaniu, więc człowiek dostawał zaproszenie
 * „No notes yet." nad katalogiem, którego nikt jeszcze nie otworzył. Zdanie było nieprawdziwe
 * dokładnie tak długo, jak trwało czytanie dysku — a przy pełnym katalogu notatek i katalogach
 * biegów nie jest to jedna klatka.
 *
 * PYTANIE ZOSTAŁO, EKRAN SIĘ ZMIENIŁ (2026-08-31). Notatki miały wtedy własną sekcję i to jej
 * ekran odpowiadał na te trzy pytania. Sekcja nazywa się dziś Knowledge, trzyma dwie półki
 * i to ONA zna trzy odpowiedzi — półka notatek nie ma już własnego zaproszenia, bo dwa
 * zaproszenia na jednym ekranie byłyby dwiema odpowiedziami na jedno pytanie (niezmiennik 13).
 * Dlatego „przeczytane" liczy się tu z OBU magazynów: jedna nieprzeczytana strona wystarczy,
 * żeby ekran nie miał prawa powiedzieć „nie ma nic".
 *
 * DRUGA POŁOWA TEJ SAMEJ WADY. Warunek pustki pytał o `notes`, `passed` i `passedProblem`,
 * a NIE pytał o `message`. Awaria odczytu notatek przy pustych przekazaniach pokazywała więc
 * zdanie o awarii NAD zaproszeniem: „Loadout could not read the notes on this machine." i zaraz
 * pod nim „No notes yet.". Dwa zdania, które nie mogą być prawdziwe naraz, i to drugie mówi
 * człowiekowi, że nie ma nic do roboty.
 *
 * # Dwie słabe wersje tego kryterium
 *
 * **Pierwsza: sprawdzić pole magazynu.** Zwrócona wartość dowodzi, że mechanizm istnieje;
 * zdanie na ekranie dowodzi, że produkt działa (niezmiennik 29). Każdy przypadek niżej
 * renderuje PRAWDZIWY ekran i czyta zdanie, które widzi człowiek.
 *
 * **Druga: sprawdzić tylko, że nowe zdanie gdzieś jest.** Przechodzi na ekranie, który mówi
 * naraz „czytam" i „nie ma nic" — czyli na dwóch odpowiedziach na jedno pytanie
 * (niezmiennik 13). Dlatego każdy przypadek pyta o obecność JEDNEGO zdania i o nieobecność
 * pozostałych.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Handoff, Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import { useSkills } from '../../state/skills';
import KnowledgeScreen from '../knowledge';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

const LIST_NOTES = 'list_notes';
const LIST_HANDOFFS = 'list_handoffs';

/** Zaproszenie: przeczytaliśmy i naprawdę nic tam nie ma. */
const NO_NOTES_YET = 'Nothing here yet.';

/** Trzecia odpowiedź, której do 2026-08-31 nie było: jeszcze nie wiemy. */
const READING = 'Reading what your agents know.';

/** Zdanie, którym Rust odmawia odczytu katalogu notatek. */
const CANNOT_READ = 'Loadout could not read the notes on this machine.';

/** Zdanie odmowy dla drugiego katalogu — innego, z innym powodem i inną rzeczą do zrobienia. */
const CANNOT_READ_PASSED = 'Loadout could not read what agents passed to each other.';

const NOTE: Note = {
  place: 'project',
  id: 'locks-and-waiting',
  title: 'Locks and waiting',
  rule: 'Never hold a lock across an await',
  because: 'One held lock and one slow read is the whole deadlock.',
  status: 'in-use',
  scope: 'this-project',
  length: 96,
  occurrences: 8,
  modified: '2026-08-31T11:30:00Z',
};

const PASSED: Handoff = {
  id: 'h-1',
  run: '0198a1f2-3b4c-7d5e-8f60-99887766aabb',
  from: 'Scout',
  to: ['Forge'],
  kind: 'findings',
  title: 'What the quote parser actually does',
  status: 'current',
  created: '2026-08-31T09:02:11Z',
  path: '/Users/someone/work/.loadout/runs/2026-08-31__abc/handoffs/02__scout__findings.md',
  bytes: 3174,
};

/** Jedna odpowiedź granicy: albo wartość, albo odmowa. */
type Answer = { readonly value: unknown } | { readonly refusal: unknown };

const queued = new Map<string, Answer>();

function willAnswer(command: string, answer: Answer): void {
  queued.set(command, answer);
}

/**
 * Stan, w jakim magazyn startuje razem z oknem.
 *
 * Czytany z niego samego, nie przepisany z ręki: pole dopisane w magazynie i pominięte tutaj
 * dałoby fiksturę, która wygląda jak świeże okno i nim nie jest.
 */
const BLANK = useMemory.getState();

/** To samo dla drugiej połowy sekcji — bez niej ekran nie umie powiedzieć „przeczytane". */
const BLANK_SKILLS = useSkills.getState();

function renderMemory(): string {
  return renderToStaticMarkup(<KnowledgeScreen notes={useMemory} skills={useSkills} />);
}

/**
 * Druga półka mówi „przeczytałam i nic tam nie ma".
 *
 * Podmiotem tego pliku są notatki, więc półka umiejętności ma być cicha — ale NIE wolno jej
 * po prostu pominąć: ekran liczy „przeczytane" z obu magazynów i nieprzeczytane umiejętności
 * trzymałyby zdanie „jeszcze czytam" także wtedy, gdy notatki dawno odpowiedziały. Fikstura
 * mówi więc wprost to, o co ten plik nie pyta.
 */
function skillsAnsweredWithNothing(): void {
  useSkills.setState({ folders: 'read', installed: [], pending: null, message: null });
}

beforeEach(() => {
  useMemory.setState(BLANK, true);
  useSkills.setState(BLANK_SKILLS, true);
  queued.clear();
  invoked.mockReset();
  invoked.mockImplementation((...sent: unknown[]) => {
    const answer = queued.get(String(sent.at(0)));
    if (answer === undefined) return Promise.resolve([]);
    return 'value' in answer ? Promise.resolve(answer.value) : Promise.reject(answer.refusal);
  });
});

describe('the section tells a person which of the three things is true', () => {
  it('says it is still reading before the disk has answered even once', () => {
    skillsAnsweredWithNothing();
    const markup = renderMemory();

    expect(
      markup,
      'the read runs in an effect, so this is the screen a person actually gets first. Telling ' +
        'them there is nothing here, over a folder nobody has opened yet, is a sentence that is ' +
        'false for as long as the read takes — and it is the sentence that says they have ' +
        'nothing to do',
    ).not.toContain(NO_NOTES_YET);
    expect(markup, 'and it says the true thing instead: nobody knows yet').toContain(READING);
  });

  it('control: turns that into the invitation once the read came back with nothing', async () => {
    skillsAnsweredWithNothing();
    await useMemory.getState().load(null);

    const markup = renderMemory();

    expect(
      markup,
      'once the folders have been read and are empty, the invitation is true and belongs on ' +
        'screen. Without this line "it says it is reading" would also pass on a section that ' +
        'says it forever',
    ).toContain(NO_NOTES_YET);
    expect(markup, 'and there is nothing left to wait for').not.toContain(READING);
  });

  it('control: shows what came back instead of either sentence', async () => {
    willAnswer(LIST_NOTES, { value: [NOTE] });
    willAnswer(LIST_HANDOFFS, { value: [PASSED] });

    skillsAnsweredWithNothing();
    await useMemory.getState().load(null);
    const markup = renderMemory();

    expect(markup, 'the note that was read is on screen').toContain(NOTE.rule);
    expect(markup, 'and neither of the two sentences about an empty screen is').not.toContain(
      NO_NOTES_YET,
    );
    expect(markup).not.toContain(READING);
  });

  it('lets a refusal to read the notes replace the invitation instead of standing over it', async () => {
    willAnswer(LIST_NOTES, { refusal: CANNOT_READ });
    willAnswer(LIST_HANDOFFS, { value: [] });

    skillsAnsweredWithNothing();
    await useMemory.getState().load(null);
    const markup = renderMemory();

    expect(
      markup,
      'the folder could not be read, so the person is owed that and only that',
    ).toContain(CANNOT_READ);
    expect(
      markup,
      'and not the invitation under it. "I could not read this" and "there is nothing here" ' +
        'cannot both be true, and the second one tells a person to stop looking',
    ).not.toContain(NO_NOTES_YET);
    expect(markup, 'nor the sentence about still reading, which is over').not.toContain(READING);
  });

  it('control: keeps the same rule for the folders the other read owns', async () => {
    willAnswer(LIST_NOTES, { value: [] });
    willAnswer(LIST_HANDOFFS, { refusal: CANNOT_READ_PASSED });

    skillsAnsweredWithNothing();
    await useMemory.getState().load(null);
    const markup = renderMemory();

    expect(
      markup,
      'these files live in a different folder, with a different reason and a different thing ' +
        'to go and do',
    ).toContain(CANNOT_READ_PASSED);
    expect(markup, 'and the invitation does not stand over that one either').not.toContain(
      NO_NOTES_YET,
    );
  });
});
