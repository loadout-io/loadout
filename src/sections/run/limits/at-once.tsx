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
  /**
   * Powód, dla którego tej liczby nie da się teraz zmienić — albo `null`, kiedy da się.
   *
   * ZDANIE, NIE `boolean`, i to jest cała treść tego pola. Wygaszona kontrolka bez powodu jest
   * zagadką: człowiek widzi suwak, którym nie da się ruszyć, i nie wie, czy to awaria. Typ
   * wymusza więc podanie zdania razem z wygaszeniem — nie da się wygasić po cichu.
   *
   * 2026-08-18 — POWSTAŁO, BO KONTROLKA KŁAMAŁA. Renderowała się zawsze czynna, a `atOnce` jest
   * czytane wyłącznie przy starcie biegu (`Limiter::new` powstaje raz i żadna komenda nie zmienia
   * limitu w trakcie). Przesunięcie z 3 na 8 w trakcie biegu zmieniało liczbę i ostrzeżenie
   * o pamięci — i nie zmieniało niczego w biegu.
   */
  disabled?: string | null;
}

export function AtOnce({
  value,
  suggested = DEFAULT_AT_ONCE,
  onChange,
  disabled = null,
}: AtOnceProps): ReactElement {
  // Przycięcie także na wejściu, nie tylko w handlerze: zapisana wartość przychodzi z pliku
  // biegu i z workflow zapisanego na większej maszynie, a suwak z `value="12"` obiecuje
  // dwunastu agentów, których limiter i tak nie wypuści.
  const atOnce = withinBounds(value ?? DEFAULT_AT_ONCE);

  return (
    /* JEDEN WIERSZ, NIE KOLUMNA — i to jest naprawa mierzona, nie estetyczna.
     *
     * 2026-08-18. Ta kontrolka stała pionowo: etykieta z liczbą, pod nią suwak, pod nim zdanie
     * „More agents finish sooner but use more memory". Razem z wyborem workflow i przyciskiem
     * Startu dawało to pas o wysokości **155 px**, stojący nad obszarem pracy PRZEZ CAŁY CZAS —
     * przy sufcie 96 px z `docs/ARCHITECTURE.md` §7 i 90 px w makiecie. Zmierzone w przeglądarce:
     * `tabBar h=34` plus ten pas `h=155` to 189 px chrome, czyli dwa razy sufit.
     *
     * Zdanie pomocy schodzi do `title`, a nie znika: mówi o KOMPROMISIE, który widać na
     * kontrolce (liczba i suwak), więc jego stała obecność kosztowała ~20 px na zawsze za
     * informację, którą czyta się raz. Ostrzeżenie o pamięci ZOSTAJE widoczne, bo mówi o
     * konkretnym ryzyku TERAZ — i nadal nie zajmuje miejsca, dopóki nie jest prawdą. */
    <div className="flex min-w-0 items-center gap-2">
      {/* PEŁNE PYTANIE ZOSTAJE W DRZEWIE, a skraca się CSS-em. Skrócenie napisu do „At once"
       * mieściło się w pasku i kasowało własność, której pilnuje kryterium `at-once.test.tsx`:
       * etykieta ma być pytaniem, jakie zadałby człowiek (DESIGN §8). Ucięcie wielokropkiem jest
       * odpowiedzią na wąskie okno; przepisanie napisu jest odpowiedzią na inne pytanie. */}
      <label className="label min-w-0 truncate" htmlFor={FIELD_ID}>
        How many agents at once?
      </label>
      {/* Liczba jest wartością maszynową, więc `.value` — kroj wchodzi razem ze stopniem
          (DESIGN §4), a `tabular-nums` trzyma ja w miejscu przy zmianie cyfry. */}
      <span className="value w-4 shrink-0 text-right" data-tone="ink">
        {atOnce}
      </span>

      <input
        id={FIELD_ID}
        type="range"
        min={MIN_AT_ONCE}
        max={MAX_AT_ONCE}
        step={1}
        value={atOnce}
        disabled={disabled !== null}
        title={disabled ?? 'More agents finish sooner but use more memory.'}
        onChange={(event: ChangeEvent<HTMLInputElement>) => {
          onChange(withinBounds(Number(event.target.value)));
        }}
        className="h-control w-24 shrink-0 accent-accent disabled:opacity-40"
      />

      {/* JEDEN SLOT NA JEDNO ZDANIE (niezmiennik 13), i te dwa zdania wykluczają się z natury:
       * w trakcie biegu liczby nie da się zmienić, więc ostrzeżenie o pamięci mówiłoby o wyborze,
       * którego w tej chwili nie ma. Powód wygaszenia ma pierwszeństwo — bo to on odpowiada na
       * pytanie, które człowiek zadaje, kiedy suwak nie chce się ruszyć.
       *
       * Przy wartości, którą maszyna unosi spokojnie, i przy suwaku, którym da się ruszyć, nie ma
       * tu ani jednego elementu — czyli zero pikseli za informację, której nie ma. */}
      {disabled === null ? (
        atOnce > suggested ? (
          <p data-at-once-warning="" className="lead fade-in min-w-0 truncate" data-tone="attend">
            {memoryWarning(atOnce)}
          </p>
        ) : null
      ) : (
        <p data-at-once-locked="" title={disabled} className="lead fade-in min-w-0 truncate">
          {disabled}
        </p>
      )}
    </div>
  );
}
