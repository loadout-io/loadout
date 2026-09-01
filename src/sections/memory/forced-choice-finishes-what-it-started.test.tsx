/* Wymuszony wybór DOMYKA to, o co człowiek poprosił — albo mówi na ekranie, dlaczego nie.
 *
 * ZMIERZONA WADA (2026-08-31). Człowiek klika „Use this", dostaje okno „Memory is full", klika
 * „Stop using" na innej notatce — i okno znika. `stopUsing` stawiało `choice: null` i NIE
 * PONAWIAŁO promocji, więc notatka, po którą przyszedł, dalej stała w strefie „Waiting for you",
 * a ani jedno zdanie na ekranie nie mówiło dlaczego. Zwolnił miejsce i nie dostał niczego:
 * z jego strony wygląda to dokładnie jak przycisk, który połknął kliknięcie.
 *
 * # Trzy słabe wersje tego kryterium
 *
 * **Pierwsza: sprawdzić `useMemory.getState().choice === null` po odstawieniu.** To był stan
 * WADLIWEGO kodu — okno znikało i wtedy właśnie zaczynał się problem. Kryterium przechodziłoby
 * na wadzie, którą ma zamykać.
 *
 * **Druga: sprawdzić, że magazyn przestawił status notatki na `in-use`.** Przechodzi na
 * magazynie, który przestawia wiersz LOKALNIE, nie pytając dysku — czyli na kłamstwie dokładnie
 * o tej jednej rzeczy, o której ta sekcja mówi: co wejdzie do promptu następnego agenta.
 * Dlatego atrapa stoi na granicy okna (`@tauri-apps/api/core`), a kryterium sądzi KOLEJNOŚĆ
 * komend, które naprawdę pojechały do Rusta: ponowienie ma być drugim zapytaniem o zapis,
 * a nie przełącznikiem w pamięci.
 *
 * **Trzecia: poprzestać na wartości zwróconej przez akcję.** Zwrócona wartość dowodzi, że
 * mechanizm istnieje; zdanie na ekranie dowodzi, że produkt działa (niezmiennik 29). Wszystkie
 * trzy przypadki niżej renderują PRAWDZIWY ekran i czytają to, co widzi człowiek: strefę „In
 * use", okno wyboru i zdanie odmowy.
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom`. Kliknięcie w „Stop
 * using" prowadzi w produkcie do tej samej akcji magazynu, którą wołamy tu wprost — okno wyboru
 * dostaje ją propsem `onStopUsing` z `src/sections/memory/shelf.tsx`.
 *
 * NAZWY KOMEND STOJĄ TU LITERAŁAMI, tak samo jak w `suggested-can-be-discarded.test.tsx` i z tego
 * samego powodu: zgodność `io.ts` ze `src-tauri/commands.golden.txt` jest osobnym pytaniem
 * i odpowiada na nie `read-paths-populate.test.ts`. Wiązanie obu w jedno dałoby jedno zdanie
 * porażki na dwie różne wady, naprawiane w dwóch różnych plikach.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { MemoryFull, Note, NoteAddress } from '../../state/memory';
import { useMemory } from '../../state/memory';
import NotesShelf from './shelf';

/* Atrapa podniesiona razem z `vi.mock`, żeby moduły sekcji dostały JĄ, a nie prawdziwy
 * transport. Cała droga magazyn → krawędź sekcji → `invoke` jedzie kodem produkcyjnym. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]): Promise<unknown> => Promise.resolve([])),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

/** Zapis „od teraz ta notatka wchodzi do promptu". */
const PUT_TO_USE = 'put_note_to_use';

/** Zapis „ta notatka przestaje wchodzić do promptu". */
const STOP_USING = 'stop_using_note';

function note(id: string, rule: string, status: Note['status'], length: number): Note {
  return {
    place: 'project',
    id,
    title: 'A rule an agent wrote down',
    rule,
    because: 'run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88',
    status,
    scope: 'this-project',
    length,
    occurrences: 2,
    modified: '2026-08-31T10:31:02Z',
  };
}

/** Notatka, po którą człowiek przyszedł: klika przy niej „Use this". */
const WANTED = note(
  'quotes-need-a-machine',
  'Prefer small state machines over hand-rolled scanning',
  'suggested',
  137,
);

/** Najdawniej używana z tych, które są w użyciu — odmowa stawia ją na liście do odstawienia. */
const OLDEST = note(
  'retry-the-flaky-suite',
  'Retry a flaky suite once before reporting',
  'in-use',
  96,
);

/** Druga w użyciu, której nikt nie rusza. Kontrola: strefa „In use" nie jest pusta z natury. */
const KEPT = note('locks-and-waiting', 'Never hold a lock across an await', 'in-use', 40);

/** Odmowa „zakres jest pełny", tak jak przyjeżdża z Rusta [T6 §5.3]. */
const FULL: MemoryFull = { overBy: 41, retire: [OLDEST.id] };

/** Ta sama odmowa po zwolnieniu miejsca: bliżej, ale wciąż za mało. */
const STILL_FULL: MemoryFull = { overBy: 12, retire: [KEPT.id] };

/** Zdanie, które Rust pisze, kiedy notatka nie ma uzasadnienia. */
const NO_REASON = 'This note has no reason on it, so it cannot go into a prompt.';

/** Katalog po odstawieniu najdawniejszej: miejsce jest, notatka wciąż czeka. */
const AFTER_STOP: Note[] = [WANTED, { ...OLDEST, status: 'suggested' }, KEPT];

/** Katalog po udanym ponowieniu: notatka, po którą człowiek przyszedł, jest w użyciu. */
const AFTER_RETRY: Note[] = [
  { ...WANTED, status: 'in-use' },
  { ...OLDEST, status: 'suggested' },
  KEPT,
];

/** Jedna odpowiedź granicy: albo wartość, albo odmowa. */
type Answer = { readonly value: unknown } | { readonly refusal: unknown };

const queued = new Map<string, Answer[]>();

/** Co ta komenda odpowie, po kolei. Bez wpisu odpowiada pustą listą. */
function willAnswer(command: string, ...answers: readonly Answer[]): void {
  queued.set(command, [...answers]);
}

/** Nazwy komend, które naprawdę pojechały do Rusta, w kolejności wysłania. */
function commandsSent(): string[] {
  return invoked.mock.calls.map((call) => String(call.at(0)));
}

function address(one: Note): NoteAddress {
  return { place: one.place, id: one.id };
}

function renderMemory(): string {
  return renderToStaticMarkup(<NotesShelf store={useMemory} />);
}

/** Kawałek markupu od znacznika tej strefy do znacznika następnej. */
function zone(markup: string, id: string): string {
  const start = markup.indexOf('data-zone="' + id + '"');
  if (start < 0) return '';
  const next = markup.slice(start + 1).search(/data-zone="/);
  return next < 0 ? markup.slice(start) : markup.slice(start, start + 1 + next);
}

/** Okno wymuszonego wyboru, albo pusty łańcuch, kiedy go na ekranie nie ma. */
function windowOf(markup: string): string {
  const at = markup.indexOf('data-choice=');
  return at < 0 ? '' : markup.slice(at);
}

beforeEach(() => {
  /* Magazyn notatek jest singletonem, więc zasianie go w jednym teście dojechałoby do
   * następnego. WSZYSTKIE pola: pominięte przecieka i pierwszym objawem jest test, który
   * przechodzi wyłącznie w swojej kolejności. */
  useMemory.setState({
    notes: [WANTED, OLDEST, KEPT],
    passed: [],
    message: null,
    passedProblem: null,
    choice: null,
  });
  queued.clear();
  invoked.mockReset();
  invoked.mockImplementation((...sent: unknown[]) => {
    const next = queued.get(String(sent.at(0)))?.shift();
    if (next === undefined) return Promise.resolve([]);
    return 'value' in next ? Promise.resolve(next.value) : Promise.reject(next.refusal);
  });
});

describe('freeing room finishes the promotion the person asked for', () => {
  it('puts the note a person asked for to use as soon as the room is free', async () => {
    willAnswer(PUT_TO_USE, { refusal: FULL }, { value: AFTER_RETRY });
    willAnswer(STOP_USING, { value: AFTER_STOP });

    await useMemory.getState().use(address(WANTED));
    expect(
      windowOf(renderMemory()),
      'control: without the window open, everything below is a statement about a screen that ' +
        'never asked the person anything',
    ).not.toBe('');

    await useMemory.getState().stopUsing(address(OLDEST));
    const after = renderMemory();

    expect(
      zone(after, 'in-use'),
      'the person clicked Use this, was told to make room, made it — and the note they came ' +
        'for has to be in the zone that reaches the model. Leaving it in Waiting for you means ' +
        'they gave something up and got nothing back, with no sentence saying why',
    ).toContain(WANTED.rule);
    expect(
      zone(after, 'suggested'),
      'and it is not still waiting as well. Two zones that both hold it are one list with two ' +
        'headings',
    ).not.toContain(WANTED.rule);
    expect(windowOf(after), 'the window closes once it has nothing left to ask about').toBe('');
    expect(
      commandsSent(),
      'and the second write really went to disk. A store that flips the row by itself once ' +
        'there is room shows In use for a note whose file still says otherwise — the one thing ' +
        'this section may never lie about (invariant 4)',
    ).toEqual([PUT_TO_USE, STOP_USING, PUT_TO_USE]);
  });

  it('keeps the window standing and names the next move when the room is still short', async () => {
    willAnswer(PUT_TO_USE, { refusal: FULL }, { refusal: STILL_FULL });
    willAnswer(STOP_USING, { value: AFTER_STOP });

    await useMemory.getState().use(address(WANTED));
    await useMemory.getState().stopUsing(address(OLDEST));
    const after = renderMemory();

    expect(
      windowOf(after),
      'one note was not enough, so the question is still open. A window that closes here leaves ' +
        'the person looking at a note that is still waiting, with nothing on screen saying so',
    ).not.toBe('');
    expect(
      windowOf(after),
      'and it says which note it is holding for them. Before this, the window named the ones ' +
        'to give up and never the one they asked for',
    ).toContain(WANTED.rule);
    expect(
      windowOf(after),
      'with the amount that is still missing, read from the fresh refusal and not from the ' +
        'first one',
    ).toContain(String(STILL_FULL.overBy));
    expect(
      windowOf(after),
      'and the list it offers is the fresh one too — the note already given up may not be ' +
        'offered a second time',
    ).not.toContain(OLDEST.rule);
  });

  it('says on screen why the note did not go in when the answer was a plain refusal', async () => {
    willAnswer(PUT_TO_USE, { refusal: FULL }, { refusal: NO_REASON });
    willAnswer(STOP_USING, { value: AFTER_STOP });

    await useMemory.getState().use(address(WANTED));
    await useMemory.getState().stopUsing(address(OLDEST));
    const after = renderMemory();

    expect(
      after,
      'the room was freed and the note still did not go in, so the screen owes the person the ' +
        'reason. Silence here is indistinguishable from a button that swallowed the click',
    ).toContain(NO_REASON);
    expect(
      windowOf(after),
      'and the window is gone: asking which notes to give up, right after saying this note ' +
        'cannot go in at all, points at the wrong thing to fix',
    ).toBe('');
  });
});
