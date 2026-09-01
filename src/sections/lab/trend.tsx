import type { ReactElement } from 'react';

/* Trend: udział przejść na przebieg, od najstarszego do najnowszego.
 *
 * ODPOWIADA NA INNE PYTANIE NIŻ TABELA i dlatego jest osobną rzeczą na ekranie. Tabela mówi
 * „jak jest teraz"; trend mówi „czy się poprawia" — i to jest pytanie, dla którego cała ta
 * sekcja powstała. Zlepienie ich w jedno dawałoby wykres, na którym nie widać, który wiersz
 * się zepsuł, i tabelę, po której nie widać, czy w ogóle jest lepiej.
 *
 * RYSUJEMY WYŁĄCZNIE TO, CO JEST W DANYCH (niezmiennik 17). Punkt na przebieg, ani jednego
 * więcej: żadnego wygładzania, żadnej linii przedłużonej „w przyszłość" i żadnej osi czasu
 * udającej równe odstępy między biegami, których nikt nie mierzył zegarem.
 *
 * BEZ WŁASNEGO KOLORU. Linia bierze `currentColor` od rodzica, tak samo jak glify nawigacji —
 * nowy kolor semantyczny jest w tym repo zakazany (`AGENTS.md` §4).
 */

/** Wysokość pola rysunku w jednostkach `viewBox`. Szerokość liczy się z liczby punktów. */
const HIGH = 24;

/** Odstęp między punktami w tych samych jednostkach. */
const APART = 16;

export interface TrendProps {
  /** Udział przejść, `0`–`1`, od najstarszego przebiegu. Krótsze niż dwa nie jest linią. */
  readonly shares: readonly number[];
}

export function Trend({ shares }: TrendProps): ReactElement | null {
  if (shares.length < 2) return null;
  const wide = APART * (shares.length - 1);
  const points = shares
    .map((share, at) => {
      const x = at * APART;
      /* Odwrócone, bo w `viewBox` zero jest na GÓRZE: udział 1 ma być wysoko, a nie nisko.
       * Wersja bez tego odwrócenia rysuje poprawną linię do góry nogami i wygląda dokładnie
       * jak regres. */
      const y = HIGH - share * HIGH;
      return String(x) + ',' + y.toFixed(1);
    })
    .join(' ');

  const latest = shares[shares.length - 1] ?? 0;
  return (
    <div data-lab-trend className="flex items-center gap-3 text-muted">
      <svg
        aria-hidden
        viewBox={'0 0 ' + String(wide) + ' ' + String(HIGH)}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        /* SZEROKOŚĆ STAŁA, wysokość stała, `preserveAspectRatio="none"`. Bez szerokości linia
           z dwóch punktów ma `viewBox` szeroki na szesnaście jednostek i kurczy się w wierszu
           do kreski, której nie da się przeczytać — a trend z dwóch przebiegów jest właśnie tym
           momentem, w którym człowiek pierwszy raz o niego pyta. */
        className="h-6 w-24 shrink-0"
        preserveAspectRatio="none"
      >
        <polyline points={points} />
      </svg>
      <span className="text-ui">
        {String(Math.round(latest * 100))}% of the last run passed, across {shares.length} runs
      </span>
    </div>
  );
}
