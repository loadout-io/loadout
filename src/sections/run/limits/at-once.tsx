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
 * OSTRZEŻENIE O PAMIĘCI JEST LICZONE, nie wpisane. Zdanie na sztywno mówi przy ośmiu
 * agentach dokładnie to samo, co przy dwóch — czyli „about 0.6 GB" w chwili, w której
 * maszyna zaczyna się dławić. Cała arytmetyka wchodzi przez jedną stałą, `MB_PER_AGENT`.
 *
 * Sufit i podłoga są atrybutami kontrolki (`min`, `max`), nie zdaniem obok niej: pole
 * liczbowe bez sufitu pozwala wpisać dziesięć i zamrozić maszynę, a napis „max 8" pod nim
 * niczego nie zatrzymuje.
 */
import type { ChangeEvent, ReactElement } from 'react';

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

/** Etykieta i suwak muszą wskazywać na siebie nawzajem, a do tego potrzebny jest identyfikator. */
const FIELD_ID = 'at-once';

/**
 * Megabajty na gigabajt — dziesiętnie, nie 1024.
 *
 * Ta liczba ma się zgadzać z tą, którą użytkownik zobaczy w Activity Monitorze, a tamten
 * liczy dziesiętnie. Przy 1024 to samo ostrzeżenie mówiłoby „2.8 GB" i różniło się od
 * jedynego miejsca, w którym da się je sprawdzić.
 */
const MB_PER_GB = 1000;

/** Ta sama podłoga i ten sam sufit, co w limiterze po stronie silnika. */
function withinBounds(atOnce: number): number {
  return Math.min(Math.max(Math.round(atOnce), MIN_AT_ONCE), MAX_AT_ONCE);
}

/** Ile pamięci wezmą agenci — jedno zdanie, z liczbą policzoną, nie wpisaną. */
function memoryWarning(atOnce: number): string {
  const gigabytes = ((atOnce * MB_PER_AGENT) / MB_PER_GB).toFixed(1);
  return `${atOnce} agents at once need about ${gigabytes} GB of memory.`;
}

export interface AtOnceProps {
  /** Zapisana wartość. Brak znaczy „nikt jeszcze nie wybierał". */
  value?: number;
  /** Podpowiedź wyliczona z pamięci maszyny. Powyżej niej kontrolka ostrzega. */
  suggested?: number;
  /** Wymagany: kontrolka bez handlera nie wchodzi do repo (niezmiennik 16). */
  onChange: (atOnce: number) => void;
}

export function AtOnce({
  value,
  suggested = DEFAULT_AT_ONCE,
  onChange,
}: AtOnceProps): ReactElement {
  // Przycięcie także na wejściu, nie tylko w handlerze: zapisana wartość przychodzi z pliku
  // biegu i z workflow zapisanego na większej maszynie, a suwak z `value="12"` obiecuje
  // dwunastu agentów, których limiter i tak nie wypuści.
  const atOnce = withinBounds(value ?? DEFAULT_AT_ONCE);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-3">
        <label className="text-label text-muted" htmlFor={FIELD_ID}>
          How many agents at once?
        </label>
        {/* Liczba jest wartością maszynową, więc mono — reguła semantyczna z DESIGN §4. */}
        <span className="font-mono text-mono text-ink">{atOnce}</span>
      </div>

      <input
        id={FIELD_ID}
        type="range"
        min={MIN_AT_ONCE}
        max={MAX_AT_ONCE}
        step={1}
        value={atOnce}
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          onChange(withinBounds(Number(event.target.value)));
        }}
        className="h-control w-full accent-accent"
      />

      <p className="text-muted">More agents finish sooner but use more memory.</p>

      {/* JEDNO ostrzeżenie i tylko powyżej podpowiedzi (niezmiennik 13). Przy wartości, którą
       * maszyna unosi spokojnie, nie ma tu pustego miejsca na ostrzeżenie — nie ma elementu. */}
      {atOnce > suggested ? (
        <p data-at-once-warning="" className="text-attend">
          {memoryWarning(atOnce)}
        </p>
      ) : null}
    </div>
  );
}
