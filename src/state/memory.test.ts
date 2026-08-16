/* Kryterium 7 dla T-17: magazyn nie kłamie o stanie i nie promuje bez odpowiedzi z Rusta.
 *
 * Słabą wersją tego kryterium jest `expect(io.putToUse).toHaveBeenCalled()`. Przechodzi na
 * magazynie, który przestawia status lokalnie i nigdy nie sprawdza odpowiedzi — czyli na takim,
 * który pokazuje „In use" także wtedy, gdy zapis się nie udał, bo zakres był pełny. Rozróżniają
 * trzy rzeczy, wszystkie w tym pliku: asercja na statusie PRZED rozwiązaniem obietnicy, asercja
 * po odrzuceniu i licznik wywołań po zamknięciu okna.
 *
 * Dlaczego akurat ten magazyn nie ma prawa być optymistyczny. Zwykły optymistyczny magazyn
 * kłamie przez 30 ms i nikt tego nie zauważa. Ten mówi o jednej rzeczy: co wejdzie do promptu
 * następnego agenta. Wiersz pokazujący „In use" przed potwierdzeniem zapisu jest kłamstwem
 * dokładnie o tej rzeczy — i zostaje kłamstwem na stałe, kiedy odpowiedź brzmi „nie".
 *
 * `vi.mock` stoi na `sections/memory/io.ts`, czyli na JEDYNYM miejscu w sekcji, które zna nazwy
 * komend (niezmiennik 23). Test tych nazw nie zna i nie ma jak ich obejść: magazyn, który
 * pojedzie do Rusta inną drogą, zostawi ten licznik na zerze i przewróci wszystko naraz.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as io from '../sections/memory/io';
import type { MemoryFull, Note } from './memory';
import { useMemory } from './memory';

vi.mock('../sections/memory/io', () => ({
  putToUse: vi.fn(),
  stopUsing: vi.fn(),
}));

const putToUse = vi.mocked(io.putToUse);
const stopUsing = vi.mocked(io.stopUsing);

const TENANT = 'tenant-before-guard';
const INDEX = 'the-index-is-disposable';
const FLAKY = 'retry-the-flaky-suite';

function note(id: string, status: Note['status']): Note {
  return {
    id,
    title: 'The tenant is resolved before the guard',
    rule: 'An unresolved tenant comes back as 401, not 400.',
    because: 'run 7f3a step 2 reproduced it in auth.e2e.spec.ts:88',
    status,
    scope: 'this-project',
    length: 137,
    occurrences: 2,
    modified: '2026-08-16T10:31:02Z',
  };
}

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

/* Obietnica, którą test trzyma otwartą. Bez niej „co widać, ZANIM Rust odpowie" nie jest
 * pytaniem, na które da się odpowiedzieć. */
function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((settle, fail) => {
    resolve = settle;
    reject = fail;
  });
  /* Pusty łapacz: odrzucenie, którego magazyn nigdy nie odebrał, ma przewrócić nazwaną
   * asercję niżej, a nie zamienić się w bezimienny błąd obok całej suity. */
  promise.catch(() => undefined);
  return { promise, resolve, reject };
}

/** Ile razy cokolwiek pojechało do Rusta. Jedna krawędź, więc jedna liczba. */
function asked(): number {
  return putToUse.mock.calls.length + stopUsing.mock.calls.length;
}

function statusOf(id: string): string {
  return useMemory.getState().notes.find((one) => one.id === id)?.status ?? 'no such note';
}

/* Magazyn jest jeden na moduł, tak jak w aplikacji. Stan początkowy bierzemy z niego samego
 * i wracamy do niego przed każdym testem — inaczej okno otwarte w jednym teście stoi otwarte
 * w następnym i suita zaczyna zależeć od kolejności. */
const BLANK = useMemory.getState();

beforeEach(() => {
  useMemory.setState(BLANK, true);
  vi.resetAllMocks();
  useMemory.setState({ notes: [note(TENANT, 'suggested'), note(INDEX, 'in-use')] });
});

/** Pyta o notatkę i dostaje odmowę „zakres jest pełny". Dwa testy zaczynają się tak samo. */
async function toldTheScopeIsFull(): Promise<MemoryFull> {
  const answer = deferred<Note>();
  putToUse.mockReturnValue(answer.promise);

  const pending = useMemory.getState().use(TENANT);
  const full: MemoryFull = { overBy: 200, retire: [INDEX, FLAKY] };
  answer.reject(full);
  await pending;

  return full;
}

describe('nothing moves in the section until Rust says it moved on disk', () => {
  it('asks once, with the id, and shows the old state until the answer arrives', async () => {
    const answer = deferred<Note>();
    putToUse.mockReturnValue(answer.promise);

    const pending = useMemory.getState().use(TENANT);

    expect(
      putToUse,
      'once, not twice: the same note written over itself would count as success both times',
    ).toHaveBeenCalledTimes(1);
    expect(putToUse, 'and it says which note it is about').toHaveBeenCalledWith({ id: TENANT });

    expect(
      statusOf(TENANT),
      'the answer is not back yet, so the row still says what is true. Anything else is a row ' +
        'saying this note reaches the model when it does not, and the refusal that follows ' +
        'never catches up with what the person already read',
    ).toBe('suggested');

    answer.resolve(note(TENANT, 'in-use'));
    await pending;

    expect(
      statusOf(TENANT),
      'and once Rust says the file moved, the row moves with it. Without this line the whole ' +
        'criterion is passed by a store that never changes anything at all',
    ).toBe('in-use');
    expect(asked(), 'and the answer did not set off a second round').toBe(1);
  });

  it('leaves a refused note alone and says in a sentence what happened', async () => {
    const answer = deferred<Note>();
    putToUse.mockReturnValue(answer.promise);

    const pending = useMemory.getState().use(TENANT);
    answer.reject(new Error('Every note needs a reason. Why is this true?'));
    await pending;

    expect(
      statusOf(TENANT),
      'the write did not happen, so the row must not say it did. This is the same lie as ' +
        'before, only permanent',
    ).toBe('suggested');

    const message = useMemory.getState().message ?? '';
    expect(
      message.split(' ').length,
      'refusing in silence looks exactly like a broken button, and the person clicks again',
    ).toBeGreaterThan(3);
    expect(
      useMemory.getState().choice,
      'an ordinary refusal is not a forced choice: opening a window that asks which note to ' +
        'give up, for a note that was refused for another reason, asks the person to fix ' +
        'something that is not broken',
    ).toBeNull();
  });

  it('opens the forced choice with the list it was given, and asks for nothing else', async () => {
    const full = await toldTheScopeIsFull();

    expect(
      useMemory.getState().choice,
      'the scope is full, so the person picks what to give up — the list comes from the ' +
        'refusal, least recently used first, and nobody rebuilds it here from what the section ' +
        'happens to hold',
    ).toEqual({ id: TENANT, overBy: full.overBy, retire: full.retire });

    expect(
      asked(),
      'and not one command more. A store that retires the first name on the list by itself is ' +
        'the silent trim this whole subsystem exists to refuse',
    ).toBe(1);
    expect(statusOf(TENANT), 'the note is still where it was').toBe('suggested');
    expect(statusOf(INDEX), 'and so is everything on the list').toBe('in-use');
  });

  it('closes the forced choice without moving anything and without asking again', async () => {
    await toldTheScopeIsFull();

    useMemory.getState().cancel();

    expect(useMemory.getState().choice, 'the window is closed').toBeNull();
    expect(
      statusOf(TENANT),
      'and the note nobody agreed to is still only suggested. Closing a window is not consent',
    ).toBe('suggested');
    expect(statusOf(INDEX), 'and the note on the list was not given up either').toBe('in-use');
    expect(
      asked(),
      'and the count stands where it stood: one ask, one refusal, no second thoughts sent to ' +
        'Rust behind the back of the person who just said no',
    ).toBe(1);
  });
});
