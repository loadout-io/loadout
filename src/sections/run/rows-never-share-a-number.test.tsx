/* Numer wiersza jest jeden na całą historię terminalu, a nie jeden na pompę.
 *
 * SKĄD TO KRYTERIUM. Audyt 2026-09-02, znalezisko F-1: `./io.ts` stemplował paczki CZTEREMA
 * osobnymi licznikami (`start`, `ask`, `asARun`, `openChat`), każdy od 1. Rozmowa i bieg jadą
 * przy tym do TEJ SAMEJ historii — `feedFor` jest kluczowane tożsamością terminalu, a nie tym,
 * co paczkę przywiozło (`./feed/live.ts`) — a ekran Pracy woła `openChat` przy każdym montażu
 * (`./index.tsx`), więc powrót na ten ekran jest trzecim nadawcą numerów 1, 2, 3 w tej samej
 * kolumnie. Model niczego przy tym nie deduplikuje po `id` (`./feed/model.ts` robi
 * `rows.push(rowFor(line))`), więc duplikat naprawdę staje w historii jako drugi wiersz.
 *
 * DLACZEGO TO NIE JEST KOSMETYKA. Numer wiersza jest jego adresem i pytają o niego trzy różne
 * rzeczy: `toggle` szuka wiersza przez `findIndex`, czyli oddaje naciśnięcie temu, kto nosi ten
 * numer PIERWSZY; `./feed/feed.tsx` stawia blok „Answered" pod wierszem, którego `id` równa się
 * `questionId` odpowiedzi; a `key` Reacta na liście wierszy jest tą samą liczbą.
 *
 * DLACZEGO PRZEZ MARKUP, A NIE PRZEZ ZWRÓCONĄ WARTOŚĆ (niezmiennik 29). Zbiór identyfikatorów
 * odczytany z modelu dowodzi, że licznik istnieje; nie dowodzi, że liczba, którą oddaje przycisk
 * `+`, trafia w wiersz, przy którym on stoi. Scena jedzie więc przez kod produkcyjny w całości:
 * prawdziwe krawędzie z `./io.ts`, prawdziwy kanał, prawdziwy model i prawdziwy komponent —
 * a numer do naciśnięcia bierzemy z `data-line`, czyli stamtąd, skąd bierze go przycisk.
 *
 * `note` JEST DOBRANY, NIE PRZYPADKOWY: nie ma go w tabeli sklejania (`./feed/model.ts`), więc
 * każda linia zostaje osobnym wierszem, i jako jedyny rodzaj niesie `body` — czyli treść, którą
 * widać w markupie dopiero wtedy, gdy wiersz jest otwarty. Wchodzi otwarty (`./feed/kinds.ts`),
 * więc naciśnięcie go ZAMYKA; mierzone jest to samo, o co chodzi: który wiersz to naciśnięcie
 * dosięgło.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';

import { Feed } from './feed/feed';
import { feedFor } from './feed/live';
import type { FeedView } from './feed/model';
import { openChat, start } from './io';

/* Atrapa transportu, podniesiona razem z `vi.mock`. Rozwiązuje się od razu, bo ta scena nie
 * mierzy odpowiedzi Rusta, tylko to, co okno robi z paczkami, które przez nią przyjechały —
 * a bieg, który nigdy nie wraca, trzymałby zapadkę `going` i drugiej sceny nie dałoby się
 * postawić. */
const { invoked, pumps } = vi.hoisted(() => ({
  invoked: vi.fn((..._sent: unknown[]) => Promise.resolve(undefined)),
  /* Kanał ZAPISUJE swój egzemplarz: każde wejście na ekran zakłada własną pompę, a scena
   * potrzebuje dokładnie tej, którą krawędź założyła przed chwilą. */
  pumps: [] as Array<{ onmessage: ((batch: unknown) => void) | null }>,
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;

    public constructor() {
      pumps.push(this);
    }
  },
}));

/* Dwa terminale, po jednym na kryterium. Rejestr strumienia nic nigdy nie zwalnia
 * (`./feed/live.ts`), więc wspólny folder dawałby drugiemu kryterium wiersze pierwszego. */
const PRESSED_IN = '/work/pressed';
const COUNTED_IN = '/work/counted';

const LEAD = 'Lead';

/** Cztery zdania jednej historii: rozmowa, dwie linie biegu i powrót na ekran Pracy. */
const TALKED = 'Read the plan and found the check that fails.';
const TALKED_BODY = 'the folder to look in was never written down anywhere';
const RAN_FIRST = 'Opened the parser.';
const RAN_SECOND = 'Wrote the missing branch.';
const CAME_BACK = 'The work is done.';
const CAME_BACK_BODY = 'two files changed and one check went from red to green';

/** Nazwa i plan dla paska — tyle, ile bierze Start, kiedy woła go ekran. */
const WHAT = { name: 'Ship a feature', steps: [] };

/** Wiersz `note` z drutu, w kształcie, którego wymaga lustro `../../ipc/types.ts`. */
function wireNote(text: string, body: readonly string[]): Record<string, unknown> {
  return { kind: 'note', agent: LEAD, text, body: [...body] };
}

/** Oddaje paczkę tą pompą, którą krawędź założyła przed chwilą — tak, jak zrobiłby to Rust. */
function deliverToTheNewest(batch: readonly unknown[]): void {
  const pump = pumps.at(-1);
  if (pump === undefined || pump.onmessage === null) {
    throw new Error('the edge opened nothing for this batch to arrive on');
  }
  pump.onmessage(batch);
}

/**
 * Rozmowa, bieg i powrót na ekran Pracy — trzy pompy, jeden terminal, jedna historia.
 *
 * Start dostaje sufit wydatku podany wprost (`null`), żeby scena nie sięgała po kwotę
 * z Settings: ta liczba nie ma z numerowaniem wierszy nic wspólnego.
 */
async function talkThenRunThenComeBack(folder: string): Promise<void> {
  const talking = openChat(folder);
  deliverToTheNewest([wireNote(TALKED, [TALKED_BODY])]);
  await talking;

  const running = start('ship-a-feature.json', 1, WHAT, folder, null, null, null);
  deliverToTheNewest([wireNote(RAN_FIRST, []), wireNote(RAN_SECOND, [])]);
  await running;

  const backAgain = openChat(folder);
  deliverToTheNewest([wireNote(CAME_BACK, [CAME_BACK_BODY])]);
  await backAgain;
}

function markupOf(view: FeedView): string {
  return renderToStaticMarkup(
    <Feed
      view={view}
      portRef={() => {
        /* Przewijanie ma swój własny plik. */
      }}
      onToggle={() => {
        /* Scena naciska model wprost, tą samą liczbą, którą oddaje przycisk. */
      }}
      onAnswer={() => {
        /* Odpowiadanie na pytanie ma swoje kryterium. */
      }}
      onJumpToNewest={() => {
        /* Skok do najnowszego wiersza też. */
      }}
    />,
  );
}

/** Numery wierszy w kolejności, w jakiej stoją w narysowanym strumieniu. */
function numbersIn(markup: string): number[] {
  return [...markup.matchAll(/data-line="(-?\d+)"/g)].map((hit) => Number(hit[1]));
}

describe('one stream hands out one number per row', () => {
  it('opens the row that was pressed, not an older one wearing its number', async () => {
    await talkThenRunThenComeBack(PRESSED_IN);
    const stream = feedFor(PRESSED_IN);
    const before = markupOf(stream.view);

    expect(
      before,
      'neither body reached the screen, so nothing here says which row a press lands on. ' +
        'Markup: ' +
        before.slice(0, 400),
    ).toContain(TALKED_BODY);
    expect(before).toContain(CAME_BACK_BODY);

    /* TĄ LICZBĄ, KTÓRĄ ODDAJE PRZYCISK: `./feed/message.tsx` woła `onToggle(row.id)`, a ten sam
     * `row.id` stoi w `data-line`. Wzięcie numeru z modelu omijałoby dokładnie tę drogę, która
     * jest tu mierzona. */
    const newest = numbersIn(before).at(-1);
    if (newest === undefined) throw new Error('the stream drew no rows at all');
    stream.toggle(newest);
    const after = markupOf(stream.view);

    expect(
      after,
      'the press was on the newest row and its own text is still on screen, so it reached a ' +
        'different one. Every pump in run/io.ts counted from one of its own, so the newest row ' +
        'wore the number 1 that the first row already had — and findIndex in feed/model.ts ' +
        'hands the press to whichever row wears it first.',
    ).not.toContain(CAME_BACK_BODY);
    expect(
      after,
      'the older row folded instead of the one that was pressed. Two rows in one history wore ' +
        'the same number, so a person pressing beside the newest sentence watches an older one ' +
        'move.',
    ).toContain(TALKED_BODY);
  });

  it('gives every row its own number across a talk, a run and a second visit', async () => {
    await talkThenRunThenComeBack(COUNTED_IN);
    const numbers = numbersIn(markupOf(feedFor(COUNTED_IN).view));

    expect(numbers.length, 'four lines went in, so four rows have to come out').toBe(4);
    expect(
      new Set(numbers).size,
      'two rows of one history came out wearing the same number: ' +
        numbers.join(', ') +
        '. A talk, a run and a second visit to the Work screen are three pumps writing to one ' +
        'column, and each of them handed out 1, 2, 3 of its own.',
    ).toBe(numbers.length);
  });
});
