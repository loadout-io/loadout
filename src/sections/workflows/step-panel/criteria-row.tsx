/* Wiersz „What it must confirm" — zatwierdzona lista wymagań tego kroku.
 *
 * # Po co to istnieje
 *
 * Zmierzone 2026-09-06, w prawdziwym biegu: krok QA opisał w prozie brak obowiązkowych zachowań
 * i w tej samej odpowiedzi napisał, że praca przeszła. Nie złamał żadnej instrukcji — pytano go,
 * czy praca jest „good enough to build on", a praca z brakującym wymaganiem naprawdę bywa dobrą
 * podstawą do dalszej pracy. Pytanie było złe, nie odpowiedź.
 *
 * Rzecz, która zmienia wynik, nie jest kolejnym akapitem promptu (niezmiennik 28): to jest LISTA,
 * którą zatwierdza CZŁOWIEK i po której Rust liczy wynik z kompletności, a nie z ostatniego
 * wiersza odpowiedzi. Bez tej kontrolki lista istniałaby wyłącznie w pliku, czyli dla nikogo.
 *
 * # Dlaczego metoda jest osobnym polem
 *
 * Bo bez niej weryfikator wolno obniża pomiar: kryterium „nagranie startuje po Later" spełnione
 * ekranem z podstawionym backendem odpowiada na inne pytanie niż to samo kryterium spełnione
 * uruchomioną aplikacją. Rust odmawia zaliczenia potwierdzenia słabszego niż zatwierdzone,
 * a to pole jest jedynym miejscem, w którym człowiek może powiedzieć, czego chce.
 *
 * Kształt wiersza jest lustrem `./handover-row.tsx` i to jest świadome: obie kontrolki są listą
 * nazwanych rzeczy z opisem, a dwa różne układy dla jednej czynności każą uczyć się jej dwa razy.
 */
import type { ReactElement } from 'react';

import type { Criterion, CriterionMethod } from '../../../state/workflows';

export interface CriteriaRowProps {
  /** Co ten krok musi dziś potwierdzić. Brak znaczy „nic nie zatwierdzono". */
  value: readonly Criterion[] | undefined;
  onEditStep: (fields: { criteria: Criterion[] | undefined }) => void;
}

const CHOICE = 'flex items-baseline gap-2 text-body text-ink';
const FIELD = 'field';
const ADD = 'label text-left hover:text-ink';

/* Brzmienia metod. Napis mówi, CZYM to potwierdzić, a nie jak nazywa się wariant na drucie
   (niezmiennik 14). */
const METHODS: readonly { readonly value: CriterionMethod; readonly label: string }[] = [
  { value: 'automated-test', label: 'An automated test' },
  { value: 'mocked-ui', label: 'The interface with stand-in data' },
  { value: 'full-runtime', label: 'The running application with its real backend' },
  { value: 'human-confirmed', label: 'A person confirming it' },
];

/** Świeże wymaganie: puste napisy, wymagane, potwierdzane testem. */
function fresh(at: number): Criterion {
  return { id: `c${at + 1}`, behaviour: '', required: true, method: 'automated-test' };
}

export function CriteriaRow({ value, onEditStep }: CriteriaRowProps): ReactElement {
  const list = value ?? [];

  const write = (next: readonly Criterion[]) => {
    /* Pusta lista wychodzi jako BRAK KLUCZA, nie jako `[]`: plik wraca wtedy do kształtu
       sprzed tego pola co do bajtu, a krok wraca do dotychczasowego kontraktu. */
    onEditStep({ criteria: next.length > 0 ? [...next] : undefined });
  };

  const edit = (at: number, change: Partial<Criterion>) => {
    write(list.map((one, index) => (index === at ? { ...one, ...change } : one)));
  };

  return (
    <div data-row="criteria" className="stack">
      <span className="label">What it must confirm</span>

      {/* ZDANIE O SKUTKU, nie o istnieniu pola. Człowiek, który zostawi tę listę pustą, dostaje
          dotychczasowe zachowanie — i ma o tym wiedzieć, zanim uzna brak wpisu za wymaganie. */}
      <span className="lead">
        Left empty, this step answers in one word at the end and that word decides. With a list, it
        has to answer about every line, and saying nothing about one of them is not a pass.
      </span>

      {list.map((one, at) => (
        <div key={at} className="stack pl-4">
          <input
            className={FIELD}
            data-field="criterion-id"
            placeholder="c1"
            value={one.id}
            onChange={(event) => {
              edit(at, { id: event.target.value });
            }}
          />
          <input
            className={FIELD}
            data-field="criterion-behaviour"
            placeholder="what a person should be able to see happen"
            value={one.behaviour}
            onChange={(event) => {
              edit(at, { behaviour: event.target.value });
            }}
          />
          <select
            className={FIELD}
            data-field="criterion-method"
            value={one.method ?? 'automated-test'}
            onChange={(event) => {
              edit(at, { method: event.target.value as CriterionMethod });
            }}
          >
            {METHODS.map((method) => (
              <option key={method.value} value={method.value}>
                {method.label}
              </option>
            ))}
          </select>
          <div className="flex items-baseline gap-3">
            <label className={CHOICE}>
              <input
                type="checkbox"
                checked={one.required !== false}
                onChange={(event) => {
                  edit(at, { required: event.target.checked });
                }}
              />
              Needed
            </label>
            <button
              type="button"
              className={ADD}
              onClick={() => {
                write(list.filter((_, index) => index !== at));
              }}
            >
              Remove
            </button>
          </div>
        </div>
      ))}

      <button
        type="button"
        className={ADD}
        onClick={() => {
          write([...list, fresh(list.length)]);
        }}
      >
        + Ask it to confirm one more
      </button>
    </div>
  );
}
