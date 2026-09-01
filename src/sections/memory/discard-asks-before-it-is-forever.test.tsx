/* „Discard" pyta, zanim skasuje na zawsze — i mówi uczciwie, że ta notatka nie wróci.
 *
 * ZMIERZONA WADA (2026-08-31). Po stronie Rusta `discard_note` zostawia TRWAŁY nagrobek
 * w `discarded/` (`src-tauri/src/memory/notes.rs`, `was_discarded`), a `scan_notes` pomija każdy
 * plik, którego slug tam stoi. Odrzucona kandydatka nie wraca NIGDY — także wtedy, gdy inny
 * agent nauczy się tego samego zdania jeszcze raz. Na ekranie było to jedno kliknięcie bez
 * pytania, obok „Use this", tym samym cichym `btn-quiet`. Nieodwracalna decyzja o odległości
 * jednego omsknięcia myszy, i ani jednego zdania o tym, co się właśnie stało.
 *
 * # Trzy słabe wersje tego kryterium
 *
 * **Pierwsza: sprawdzić, że gdzieś w markupie stoi napis „Discard for good".** Przechodzi na
 * napisie w `<span>`, który wygląda tak samo, a klika się zupełnie inaczej — i przechodzi też
 * na wierszu, który rysuje pytanie ZAWSZE. Odróżniają dwie rzeczy: wycięty otwierający znacznik
 * przycisku wokół etykiety oraz kontrola „przed kliknięciem tego pytania na ekranie nie ma".
 *
 * **Druga: sprawdzić stan magazynu po zapytaniu.** Zwrócona wartość dowodzi, że mechanizm
 * istnieje; zdanie na ekranie dowodzi, że produkt działa (niezmiennik 29). Każdy przypadek
 * niżej renderuje PRAWDZIWY ekran i czyta wiersz, który widzi człowiek.
 *
 * **Trzecia: poprzestać na tym, że pytanie się pokazuje.** Pytanie, które przy okazji już
 * skasowało plik, jest gorsze niż brak pytania: człowiek czyta „na pewno?" nad rzeczą, której
 * nie ma. Dlatego kryterium sądzi też TAŚMĘ wywołań — postawienie pytania nie ma prawa wysłać
 * do Rusta ani jednego zapisu.
 *
 * WZORZEC DWUSTOPNIOWEGO PYTANIA STOI W `src/sections/agents/index.tsx` i stąd jest wzięty:
 * potwierdzenie jest PRAWDZIWYM RENDEREM, nie `window.confirm`. Dialog przeglądarki blokuje
 * webview i zabiera całą sesję pracy, a przy oknie Tauri nie ma go czym odblokować.
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom`. Kliknięcie prowadzi
 * w produkcie do tych samych akcji magazynu, które wołamy tu wprost — wiersz dostaje je propsami
 * z `src/sections/memory/shelf.tsx`.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { MemoryState, Note, NoteAddress } from '../../state/memory';
import { useMemory } from '../../state/memory';
import NotesShelf from './shelf';

const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

/** Nazwa komendy po stronie Rusta — ta sama, którą sądzi `suggested-can-be-discarded.test.tsx`. */
const DISCARD_COMMAND = 'discard_note';

/** Etykieta pierwszego kliknięcia. Zostaje jednym słowem, tak jak w makiecie. */
const DISCARD = 'Discard';

/** Etykieta drugiego. Mówi, że to jest koniec drogi, a nie powtórzenie pierwszego. */
const FOR_GOOD = 'Discard for good';

/** Wyjście z pytania. */
const KEEP_IT = 'Keep it';

/**
 * Zdanie, które musi paść, zanim notatka odejdzie na zawsze.
 *
 * Sądzony jest FRAGMENT, a nie całe zdanie znak w znak: kryterium ma pilnować, żeby człowiek
 * usłyszał o NIEODWRACALNOŚCI, a nie zamrażać brzmienie copy (niezmiennik 20).
 */
const NOT_COMING_BACK = 'will not come back';

function note(id: string, rule: string, status: Note['status']): Note {
  return {
    place: 'project',
    id,
    title: 'A rule an agent wrote down',
    rule,
    because: 'run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88',
    status,
    scope: 'this-project',
    length: 137,
    occurrences: 3,
    modified: '2026-08-31T09:00:00Z',
  };
}

const WAITING = note(
  'quotes-need-a-machine',
  'Prefer small state machines over hand-rolled scanning',
  'suggested',
);
const SECOND = note(
  'retry-the-flaky-suite',
  'Retry a flaky suite once before reporting',
  'suggested',
);
const IN_USE = note('locks-and-waiting', 'Never hold a lock across an await', 'in-use');

function address(one: Note): NoteAddress {
  return { place: one.place, id: one.id };
}

function renderMemory(): string {
  return renderToStaticMarkup(<NotesShelf store={useMemory} />);
}

/** Kawałek markupu jednego wiersza — od jego adresu do końca elementu listy. */
function row(markup: string, one: Note): string {
  const start = markup.indexOf('data-note-address="' + one.place + ':' + one.id + '"');
  if (start < 0) return '';
  const end = markup.indexOf('</li>', start);
  return end < 0 ? markup.slice(start) : markup.slice(start, end);
}

/**
 * Otwierający znacznik przycisku niosącego tę etykietę. Brak etykiety jest tu porażką, a nie
 * cichym `undefined`: napis w `<span>` wygląda w markupie tak samo jak przycisk.
 */
function buttonFor(html: string, label: string): string {
  const at = html.indexOf(label);
  if (at < 0) {
    throw new Error('nothing on screen is labelled: ' + label);
  }
  const opens = html.lastIndexOf('<button', at);
  if (opens < 0) {
    throw new Error('this label is not inside a button: ' + label);
  }
  return html.slice(opens, html.indexOf('>', opens) + 1);
}

/** Nazwy komend, które naprawdę pojechały do Rusta, w kolejności wysłania. */
function commandsSent(): string[] {
  return invoked.mock.calls.map((call) => String(call.at(0)));
}

/**
 * Akcje dwustopniowego pytania, czytane przez rozszerzenie typu, a nie wołane wprost.
 *
 * Dzięki temu brak tej ścieżki jest NAZWANĄ porażką ze zdaniem o ekranie, a nie tym samym
 * `TypeError`, którym odmawia wywołanie czegokolwiek, czego nie ma.
 */
function askFrom(state: MemoryState): ((address: NoteAddress) => void) | undefined {
  return state.askDiscard;
}

function keepFrom(state: MemoryState): (() => void) | undefined {
  return state.keepIt;
}

beforeEach(() => {
  useMemory.setState({
    notes: [WAITING, SECOND, IN_USE],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
    pendingDiscard: null,
  });
  invoked.mockReset();
  invoked.mockImplementation(() => Promise.resolve([]));
});

describe('throwing a note away for good is asked about first', () => {
  it('control: the row offers Discard and stands no question over it until it is clicked', () => {
    const markup = renderMemory();

    expect(
      buttonFor(row(markup, WAITING), DISCARD),
      'the first decision stays exactly where it was. Without this line every case below also ' +
        'passes on a screen that lost the control altogether',
    ).toContain('<button');
    expect(
      markup,
      'and no question stands on a screen nobody has clicked. A row that draws it always would ' +
        'pass the case below without asking anybody anything',
    ).not.toContain(NOT_COMING_BACK);
  });

  it('asks first, and says out loud that the note is not coming back', () => {
    const ask = askFrom(useMemory.getState());
    ask?.(address(WAITING));

    const asking = renderMemory();
    const asked = row(asking, WAITING);

    expect(
      asked,
      'one click on Discard puts a permanent tombstone on disk and this note is never suggested ' +
        'again, not even after another agent learns it. A person owes a question before that, ' +
        'and the question owes them the part they cannot see: it does not come back',
    ).toContain(NOT_COMING_BACK);
    expect(
      buttonFor(asked, FOR_GOOD),
      'and the way through is a real control, not a sentence about one',
    ).toContain('<button');
    expect(
      buttonFor(asked, KEEP_IT),
      'with a way out beside it. A question with one answer is not a question',
    ).toContain('<button');
    expect(
      commandsSent(),
      'asking is not doing. A question standing over a file that is already gone is worse than ' +
        'no question at all',
    ).toEqual([]);
  });

  it('stands that question over one row at a time, and leaves the other zone alone', () => {
    askFrom(useMemory.getState())?.(address(WAITING));
    const asking = renderMemory();

    expect(
      row(asking, WAITING),
      'the row that was clicked is the one being asked about. Without this line the whole case ' +
        'below is also passed by a screen that asks nobody anything',
    ).toContain(NOT_COMING_BACK);
    expect(
      row(asking, SECOND),
      'the second candidate is not being asked about, so it keeps its plain row. One question ' +
        'in one place (invariant 13)',
    ).not.toContain(NOT_COMING_BACK);
    expect(
      row(asking, IN_USE),
      'and a note that is already reaching the model is not thrown away from here at all',
    ).not.toContain(DISCARD);
  });

  it('takes the question away on Keep it, without sending anything to disk', () => {
    askFrom(useMemory.getState())?.(address(WAITING));
    expect(
      renderMemory(),
      'the question has to be standing before this case can say anything about closing it. ' +
        'Without this line "no question on screen" passes on a screen that never had one',
    ).toContain(NOT_COMING_BACK);

    keepFrom(useMemory.getState())?.();

    const after = renderMemory();

    expect(
      after,
      'saying no closes the question. A question that stays on screen after Keep it teaches ' +
        'people to click through it',
    ).not.toContain(NOT_COMING_BACK);
    expect(
      buttonFor(row(after, WAITING), DISCARD),
      'and the row is back the way it was, with the decision still available',
    ).toContain('<button');
    expect(commandsSent(), 'and nothing at all was written').toEqual([]);
  });

  it('goes through to disk only on the second click, and the row leaves the screen', async () => {
    askFrom(useMemory.getState())?.(address(WAITING));
    expect(
      renderMemory(),
      'the question has to be standing first, or the last line of this case says nothing about ' +
        'what happens to it',
    ).toContain(NOT_COMING_BACK);

    await useMemory.getState().discard(address(WAITING));

    const after = renderMemory();

    expect(
      commandsSent(),
      'the second click is the one that writes. A question in front of a control that never ' +
        'reaches disk is theatre',
    ).toEqual([DISCARD_COMMAND]);
    expect(
      row(after, WAITING),
      'and the row is gone, because Rust answered with the fresh catalogue',
    ).toBe('');
    expect(
      after,
      'and the question goes with it. A question left standing over a row that is no longer ' +
        'there asks about nothing',
    ).not.toContain(NOT_COMING_BACK);
  });
});
