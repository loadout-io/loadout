/* Kryterium 6 dla T-92: kandydatka daje się odrzucić z ekranu, a notatka w użyciu — nie tędy.
 *
 * ZMIERZONA WADA, KTÓRĄ TO ZAMYKA. Do 2026-08-23 pamięć miała na ekranie DOKŁADNIE JEDNO
 * wejście dla decyzji człowieka i było nim „tak". Makieta rysuje przy kandydatce dwie akcje
 * (`docs/mockup/index.html:757`), `NoteRow` renderuje jedną, a `MemoryState` zna `use`,
 * `stopUsing` i `cancel` — czyli człowiek, któremu agent zaproponował zdanie nieprawdziwe, nie
 * miał ani jednej drogi, żeby to powiedzieć. `src/sections/memory/mounted.test.tsx` nazywa tę
 * lukę wprost i zostawia ją człowiekowi, bo oba potrzebne pliki leżały wtedy poza jego blokiem
 * OWNS. Teraz leżą w tym.
 *
 * # Trzy słabe wersje tego kryterium
 *
 * **Pierwsza: sprawdzić, że słowo „Discard" jest gdzieś w markupie.** Przechodzi na napisie
 * w `<span>`, który wygląda tak samo, a klika się zupełnie inaczej. Dlatego niżej wycinany jest
 * otwierający znacznik przycisku wokół etykiety — ta sama technika, co w `note-row.test.tsx`.
 *
 * **Druga: renderować jeden wariant.** Przechodzi na wierszu, który ma ten przycisk wpisany na
 * sztywno dla obu stanów — a odrzucenie notatki, która właśnie jedzie do promptu, jest tą jedną
 * rzeczą, której ten przycisk robić NIE MA. Oba warianty pochodzą więc z tego samego wiersza,
 * a drugi ma osobną kontrolę przeciw pustemu renderowi.
 *
 * **Trzecia: zawołać krawędź sekcji wprost i zobaczyć, co wróciło.** Odpowiada wtedy na pytanie
 * „czy funkcja działa", a pytanie brzmi „czy ekran ma czym to zrobić". Dlatego wołana jest
 * wyłącznie akcja magazynu, sądzony jest STAN magazynu, a atrapa stoi dopiero na granicy okna:
 * cała droga magazyn → krawędź sekcji → `invoke` jedzie kodem produkcyjnym.
 *
 * DLACZEGO NAZWA KOMENDY STOI TU LITERAŁEM. `src-tauri/commands.golden.txt` jest jedyną listą,
 * którą czytają obie strony granicy — i to kryterium 3 tego zadania wymaga, żeby ta nazwa się
 * na niej znalazła. Wybieranie jej stąd wiązałoby dwa kryteria w jedno: sekcja bez przycisku
 * i lista bez wiersza dawałyby jedno zdanie, a naprawia się je w dwóch różnych plikach.
 *
 * Render jest statyczny (`renderToStaticMarkup`), bo w repo nie ma `jsdom` — zachowanie jedzie
 * przez akcję magazynu, dokładnie tak, jak dzieli to `tasks/T-92.md`.
 */
import type { ComponentProps, ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { MemoryState, Note } from '../../state/memory';
import { useMemory } from '../../state/memory';
import MemoryScreen from './index';
import { NoteRow } from './note-row';

/* Atrapa podniesiona razem z `vi.mock`, żeby moduły sekcji dostały JĄ, a nie prawdziwy
 * transport. Zachowanie ustawia każdy przypadek osobno. */
const { invoked } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]): Promise<unknown> => Promise.resolve(undefined)),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke: invoked }));

/** Nazwa komendy po stronie Rusta — patrz nagłówek. */
const COMMAND = 'discard_note';

/** Etykieta drugiej akcji kandydatki. Jedno słowo, tak jak w makiecie. */
const DISCARD = 'Discard';

/** Etykieta pierwszej. Stoi tu, bo obie mają być w wierszu naraz. */
const USE = 'Use this';

/** Etykieta wiersza w użyciu — kontrola przeciw pustemu renderowi. */
const STOP = 'Stop using';

/** Kandydatka: agent ją zaproponował, człowiek jeszcze nie powiedział „tak". */
const WAITING: Note = {
  id: 'n-1',
  title: 'Quote handling needs a state machine',
  rule: 'Prefer small state machines over hand-rolled scanning',
  because: 'Character-by-character checks miss embedded separators.',
  status: 'suggested',
  scope: 'this-project',
  length: 137,
  occurrences: 3,
  modified: '2026-08-23T09:00:00Z',
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
  modified: '2026-08-21T11:30:00Z',
};

function noop(): void {
  /* sterowany wiersz: w statycznym renderze nic tego nie woła */
}

/* Wiersz z prop-em, którego dziś jeszcze nie deklaruje. Kontrolka bez handlera nie wchodzi do
 * repo (niezmiennik 16), więc przycisk „Discard" musi mieć swój — a `note-row.test.tsx` z T-17
 * leży poza blokiem OWNS tego zadania i nie ma prawa przestać się kompilować. */
type RowProps = ComponentProps<typeof NoteRow> & { onDiscard: (id: string) => void };
const Row = NoteRow as (props: RowProps) => ReactElement;

function markup(status: Note['status']): string {
  const note = status === 'suggested' ? WAITING : IN_USE;
  return renderToStaticMarkup(<Row note={note} onUse={noop} onStopUse={noop} onDiscard={noop} />);
}

/**
 * Otwierający znacznik przycisku niosącego tę etykietę. Brak etykiety jest tu porażką, a nie
 * cichym `undefined`: napis w `<span>` wygląda w markupie tak samo jak przycisk.
 */
function buttonFor(html: string, label: string): string {
  const at = html.indexOf(label);
  if (at < 0) {
    throw new Error('the row shows nothing labelled: ' + label);
  }
  const opens = html.lastIndexOf('<button', at);
  if (opens < 0) {
    throw new Error('this label is not inside a button: ' + label);
  }
  return html.slice(opens, html.indexOf('>', opens) + 1);
}

/** Kawałek markupu od znacznika tej strefy do znacznika następnej. */
function zone(html: string, id: string): string {
  const start = html.indexOf('data-zone="' + id + '"');
  if (start < 0) return '';
  const next = html.slice(start + 1).search(/data-zone="/);
  return next < 0 ? html.slice(start) : html.slice(start, start + 1 + next);
}

/** Identyfikatory notatek, które sekcja trzyma w tej chwili. */
function idsOnScreen(): string[] {
  return useMemory.getState().notes.map((one) => one.id);
}

/**
 * Akcja „Discard" z magazynu, albo nic.
 *
 * Czytana przez rozszerzenie typu, a nie wołana wprost, żeby brak tej ścieżki był NAZWANĄ
 * porażką z własnym zdaniem — a nie tym samym zdaniem, którym odmawia każde wywołanie czegoś,
 * czego nie ma.
 */
function discardFrom(state: MemoryState): ((id: string) => Promise<void>) | undefined {
  return (state as MemoryState & { discard?: (id: string) => Promise<void> }).discard;
}

beforeEach(() => {
  /* Magazyn notatek jest singletonem, więc zasianie go w jednym teście dojechałoby do
   * następnego. WSZYSTKIE pola: pominięte przecieka między testami, a pierwszym objawem jest
   * test przechodzący wyłącznie w swojej kolejności. */
  useMemory.setState({ notes: [], passed: [], message: null, passedProblem: null, choice: null });
  invoked.mockReset();
  invoked.mockImplementation(() => Promise.resolve(undefined));
});

describe('a suggested note can be thrown away, and one in use cannot be thrown away from here', () => {
  it('puts both of the two decisions a person can make in front of a suggested note', () => {
    const html = markup('suggested');

    expect(
      buttonFor(html, USE),
      'the first decision stays exactly where it was. Without this line the whole check below ' +
        'is also passed by a row that swapped one action for the other, which takes away the ' +
        'only way a note ever reaches the model',
    ).toContain('<button');
    expect(
      buttonFor(html, DISCARD),
      'and the second decision is a real control, not a sentence about one. A person whose ' +
        'agent proposed something untrue had no way at all to say so: the list only ever grew, ' +
        'and a list nothing ever leaves stops being read [T6 §5.1]',
    ).toContain('<button');
  });

  it('leaves a note that is already reaching the model out of it', () => {
    const html = markup('in-use');

    expect(
      buttonFor(html, STOP),
      'the same row, the other state, and the way back is still there. The control against an ' +
        'empty assertion: without it the line below also passes on a row that rendered nothing',
    ).toContain('<button');
    expect(
      html,
      'throwing away a note that is in use is a second question wearing the first one’s ' +
        'clothes. It leaves in one click from the place a person was looking for it, and they ' +
        'asked for one thing: stop using it first, then decide whether it goes',
    ).not.toContain(DISCARD);
  });

  it('draws that button in the zone that waits for a person, and nowhere else', () => {
    useMemory.setState({ notes: [WAITING, IN_USE] });

    const html = renderToStaticMarkup(<MemoryScreen store={useMemory} />);

    expect(
      zone(html, 'suggested'),
      'the row is one thing and the screen putting it there is another. A row nobody hands ' +
        'the second handler to draws the same button and refuses every click',
    ).toContain(DISCARD);
    expect(
      zone(html, 'in-use'),
      'and the other zone does not carry it, on the screen as well as in the row',
    ).not.toContain(DISCARD);
  });

  it('asks Rust first and takes the row away only once the answer is back', async () => {
    useMemory.setState({ notes: [WAITING, IN_USE] });
    const discard = discardFrom(useMemory.getState());
    expect(
      typeof discard,
      'the memory store has no way to throw a candidate away. The button in the row has ' +
        'nothing to call, and a control without a handler does not enter the repo ' +
        '(invariant 16)',
    ).toBe('function');

    /* Odpowiedź zatrzymana w połowie: bez niej „bez optymistycznej aktualizacji" jest zdaniem
     * o kolejności, której nikt nie sprawdził. */
    let answer: (value: unknown) => void = () => undefined;
    invoked.mockImplementation(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );

    const pending = discard?.(WAITING.id);

    expect(
      invoked.mock.calls,
      'one question asked, and it names the command and carries the note it is about. A row ' +
        'that takes itself off the list without asking is the same silent trimming this whole ' +
        'subsystem exists to refuse [T6 §5.3]',
    ).toEqual([[COMMAND, { id: WAITING.id }]]);
    expect(
      idsOnScreen(),
      'the row is still there while the answer is on its way. The rest of this screen already ' +
        'works this way, and for the same reason: a row that leaves before the write landed ' +
        'lies about the one thing this section is about — what is on disk (invariant 4)',
    ).toEqual([WAITING.id, IN_USE.id]);

    answer(undefined);
    await pending;

    expect(
      idsOnScreen(),
      'and once the answer is back it is gone — that one, and only that one. A candidate ' +
        'a person turned down that stays on the list turns the whole list into something they ' +
        'read past',
    ).toEqual([IN_USE.id]);
  });
});
