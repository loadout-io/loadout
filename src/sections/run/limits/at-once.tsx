/* Kontrolka „How many agents at once?".
 *
 * Czysta funkcja stanu na markup: nie trzyma własnego stanu i nie wie, że istnieje `invoke()`.
 * Podłączenie jej do biegu należy do T-07 i T-15 — tutaj powstaje sama kontrolka.
 *
 * `onChange` jest WYMAGANY (niezmiennik 16). Suwak, który renderuje się ładnie i nie woła
 * niczego, jest gorszy niż jego brak, bo obiecuje sterowanie; poprzedni prototyp ma trzy takie
 * przyciski. Skoro komenda do zmiany limitu w trakcie biegu jeszcze nie istnieje, brak
 * handlera ma być błędem TYPÓW, nie martwą kontrolką w repo.
 *
 * STAN TEGO PLIKU: SZKIELET (2026-08-16). Komponent zwraca pusty element, żeby kryterium
 * padło na asercji, a nie na braku modułu: vitest przewraca się już na ZBIERANIU plików,
 * a „Cannot find module" nie liczy się jako czerwone (AGENTS.md §2a p. 5).
 */
import type { ReactElement } from 'react';

/** Ile agentów naraz, kiedy nikt jeszcze nic nie wybrał. */
export const DEFAULT_AT_ONCE = 3;

/** Podłoga suwaka. Zero agentów to bieg, który nigdy nie ruszy. */
export const MIN_AT_ONCE = 1;

/** Sufit suwaka. Powyżej ośmiu wiąże limit u dostawcy, nie pamięć. */
export const MAX_AT_ONCE = 8;

/**
 * Ile pamięci bierze jeden agent, w megabajtach.
 *
 * Zmierzone: 583 MB szczytowego RSS jednego drzewa `claude` [T7 §7.1, V]. Z tej jednej liczby
 * liczy się ostrzeżenie — zdanie wpisane na sztywno mówi przy ośmiu agentach to samo, co przy
 * dwóch, i dlatego nie ostrzega przed niczym.
 */
export const MB_PER_AGENT = 583;

export interface AtOnceProps {
  /** Zapisana wartość. Brak znaczy „nikt jeszcze nie wybierał". */
  value?: number;
  /** Podpowiedź wyliczona z pamięci maszyny. Powyżej niej kontrolka ostrzega. */
  suggested?: number;
  /** Wymagany: kontrolka bez handlera nie wchodzi do repo (niezmiennik 16). */
  onChange: (atOnce: number) => void;
}

export function AtOnce(props: AtOnceProps): ReactElement {
  // SZKIELET — pusty element. Props są tu odczytane wyłącznie po to, żeby bramka typów nie
  // zgłosiła nieużywanego parametru; żadna z tych wartości nie dociera do markupu i o to
  // chodzi, bo kryterium ma być czerwone od braku ZACHOWANIA.
  void props;
  return <div />;
}
