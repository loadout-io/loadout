/* Każda linia, którą wysyłasz, zostawia wiersz — i ten wiersz nie udaje zdarzenia biegu.
 *
 * Zgłoszenie właściciela 2026-08-20: komendy nie zostawiają po sobie ani jednego wiersza.
 * Cicha porażka, przed którą stoi ten plik: terminal, w którym wpisana komenda nie zostawia
 * śladu, jest nieodróżnialny od terminala, który tej komendy nie przyjął.
 *
 * SŁABA WERSJA: sprawdzenie, że tekst jest niepusty. Przechodzi dla implementacji, która składa
 * wiersz podpisany agentem i z identyfikatorem 1 — czyli takiej, która psuje klucze Reacta
 * (dwie pompy w `../io.ts` stemplują od 1 każda) i przypisuje Twoje słowa komuś innemu.
 * Rozróżniają to dwa przypadki niżej: numer i podpis.
 */
import { describe, expect, it } from 'vitest';

import { createFeed } from '../feed/model';
import type { Scroller } from '../feed/model';
import { authorityOf } from '../rail/say';
import { echoOf, saidOf } from './echo';
import type { WindowLine } from './echo';
import { NOTHING_RUNS, NOT_KNOWN } from './entry';

/** Linia z przykładu właściciela: nazwa workflow plus zadanie, czyli cała reszta wiersza. */
const TYPED = '/run easy zbuduj X';

/**
 * Port przewijania, który nigdzie nie jeździ.
 *
 * Model widoku bierze go w konstruktorze, a ten plik nie ma o przewijaniu nic do powiedzenia
 * — pyta tylko, czy złożony wiersz wchodzi do historii.
 */
const NO_SCROLLING: Scroller = {
  scrollTop: () => 0,
  scrollTo: () => undefined,
  scrollIntoView: () => undefined,
};

/**
 * Zdania wierszy, które ten wiersz WSTAWIŁ do historii — przepuszczone przez jedyne drzwi,
 * jakie okno ma do strumienia (`appendLines`).
 *
 * Pytamy o to, co z tych drzwi wychodzi, a nie o rodzaj, który do nich wszedł: rodzaj wybiera
 * implementacja, a kryterium ma sprawdzać skutek. Wiersz rodzaju, którego model nie zna, jest
 * PORZUCANY w ciszy — i wtedy ta lista jest pusta, choć obiekt istniał.
 */
function intoTheStream(row: WindowLine): readonly string[] {
  return createFeed(NO_SCROLLING)
    .appendLines([row])
    .map((entered) => entered.label);
}

describe('every line you send leaves a row, and none of them pretends to come from the run', () => {
  it('carries the line you sent, character for character, all the way into the stream', () => {
    const row = echoOf(TYPED);
    expect(
      row,
      'a command has to leave a row behind. Without one, a line that was obeyed and a line that ' +
        'was never read look exactly the same on the screen — and that is the whole defect this ' +
        'file exists to close.',
    ).not.toBeNull();
    if (row === null) return;

    expect(
      row.text,
      'the row has to carry the WHOLE line, not the command word. Half of what a person typed ' +
        'is worse than nothing here: it reads as if Loadout heard something else than what was ' +
        'written, and there is no second place to check.',
    ).toContain(TYPED);

    /* Wiersz, którego historia nie przyjmuje, nie jest wierszem — jest obiektem. Pytamy więc
     * jedynymi drzwiami, jakie okno ma do strumienia, i o tekst, który z nich wychodzi: rodzaj
     * sklejany w liczniku („Read 3 files") zgubiłby tu wpisane zdanie. */
    const labels = intoTheStream(row);
    expect(
      labels.length,
      'the composed row has to be a row the stream accepts into its history. A shape the model ' +
        'drops is silently no row at all — that is how an unknown kind behaves, by design.',
    ).toBe(1);
    expect(
      labels[0] ?? '',
      'and the row that entered the history has to say what was typed. A kind that folds into a ' +
        'counter would swallow the sentence and leave "1 file" where a command used to be.',
    ).toContain(TYPED);
  });

  it('puts the answers of the line itself into that same one history', () => {
    /* Trzy odpowiedzi, jakie ten wiersz daje. Dwie pierwsze są stałymi (`./entry`), trzecia
     * przyjeżdża w czasie biegu z `../run-command.ts` i dlatego stoi tu jako DOWOLNE zdanie:
     * moduł, który składa wiersz tylko dla dwóch znanych napisów, przechodzi to kryterium
     * i gubi odmowę startu — czyli jedyną z tych trzech, która kosztuje pieniądze. */
    const refusedToStart = 'There is no workflow with steps in it yet.';

    for (const sentence of [NOT_KNOWN, NOTHING_RUNS, refusedToStart]) {
      const row = saidOf(sentence);
      expect(
        row.text,
        'the answer has to carry its own sentence, word for word — this text is already written ' +
          'for a person to read, here and in the refusals Rust sends back, so anything added to ' +
          'it is copy nobody wrote: ' +
          JSON.stringify(sentence),
      ).toBe(sentence);
      expect(
        intoTheStream(row),
        'the answer has to land in the SAME history as everything else, through the same door. ' +
          'Talking with Loadout is one story, not half of it under the field: the version under ' +
          'the field shows the last sentence only, so three answers in a row leave two of them ' +
          'unseen — and none of them survives the next line. Sentence: ' +
          JSON.stringify(sentence),
      ).toEqual([sentence]);
    }
  });

  it('numbers its rows below zero, and never twice the same', () => {
    const rows = [echoOf('/stop'), saidOf(NOTHING_RUNS), echoOf('/open')];
    const numbers: number[] = [];
    for (const row of rows) {
      expect(row, 'each of these lines has to leave a row to be numbered').not.toBeNull();
      if (row === null) continue;
      expect(
        row.id,
        'the number has to be below zero. Both pumps on the boundary stamp from 1 and they do it ' +
          'separately — the run in `start()` and the conversation in `openChat()` — so a positive ' +
          'counter in the window collides with theirs inside one window. Two rows under one ' +
          'number are one row to React, and the older of the two is the one that disappears.',
      ).toBeLessThan(0);
      numbers.push(row.id);
    }
    expect(
      new Set(numbers).size,
      'and no two rows may share a number: ' +
        JSON.stringify(numbers) +
        '. React keys off it, so a repeat means one of the two lines never reaches the screen.',
    ).toBe(numbers.length);
  });

  it('signs the row Loadout, asked by calling the one function that decides that', () => {
    const rows = [echoOf(TYPED), saidOf(NOT_KNOWN)];
    for (const row of rows) {
      expect(row, 'there has to be a row before there is a signature under it').not.toBeNull();
      if (row === null) continue;
      /* WOŁAMY tę funkcję, a nie zakładamy nic o rodzaju: „kto to powiedział" mieszka w jednym
       * miejscu (`../rail/say.ts`) i tylko pytanie jej jest pytaniem o prawdę. Wiersz podpisany
       * `agent` byłby cytatem przypisanym komuś, kto go nie wypowiedział, a podpisany `you`
       * udawałby zdanie z drutu, którego w `run.json` nie ma. */
      expect(
        authorityOf(row.kind),
        'the row is written by Loadout: the person typed a command, and the answer to it is ' +
          'Loadout speaking. Signed `agent` it becomes a quote from somebody who never said it; ' +
          'signed `you` it borrows the mark that belongs to the prose the wire really carries.',
      ).toBe('loadout');
    }
  });

  it('leaves prose alone, because its row comes from the wire', () => {
    for (const prose of ['tell me what is going on', 'rewrite the reader, it is too strict']) {
      expect(
        echoOf(prose),
        'a sentence without a slash must NOT get a row from the window: it already has one from ' +
          'the wire (`told`, signed `You →`). Two rows for one sentence are two places of truth ' +
          'about the same thing, and the local one would be the one that disappears on reload.',
      ).toBeNull();
    }
  });
});
