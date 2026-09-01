import type { ReactElement } from 'react';

/* Glify nawigacji. Szesc, po jednym na sekcje, 16 px, obrys, `currentColor`.
 *
 * GRAMATYKA JEST TRESCIA, NIE STYLEM, i jest sprawdzana (T-46 AC-5):
 *
 *   wezly i krawedzie  ->  rzecz, ktora JEST grafem            (Workflows)
 *   plyty w stosie     ->  rzecz, ktora jest ZBIOREM           (Agents, Knowledge)
 *   trojkat            ->  jedyna rzecz, ktora sie DZIEJE      (Run)
 *
 * To niezmiennik 17 przeniesiony na ikonografie: nie rysujemy relacji tam, gdzie relacji
 * nie ma. Ikona Agents z dwoma okregami polaczonymi linia obiecywalaby zaleznosc miedzy
 * agentami, ktorej w danych nie ma — dokladnie tak samo jak ozdobna krzywa miedzy
 * zakodowanymi na sztywno wspolrzednymi na plotnie.
 *
 * ZADEN glif nie niesie wlasnej barwy. Aktywny bierze akcent, pozostale przygaszony tekst,
 * a jedno i drugie idzie przez `currentColor` z rodzica — bo ktora sekcja jest otwarta, jest
 * powiedziane DOKLADNIE RAZ, atrybutem `aria-current` (niezmiennik 13).
 */

const PATHS: Readonly<Record<string, readonly ReactElement[]>> = {
  /* Jedyna rzecz, ktora sie dzieje: jedna zamknieta sciezka. */
  run: [<path key="t" d="M4 3.6 L12.6 8 L4 12.4 Z" />],
  /* Graf: trzy wezly i dwie krawedzie. Jedyny glif, ktoremu wolno je miec. */
  workflows: [
    <circle key="a" cx="3.6" cy="8" r="1.7" />,
    <circle key="b" cx="12.4" cy="4.2" r="1.7" />,
    <circle key="c" cx="12.4" cy="11.8" r="1.7" />,
    <path key="e" d="M5.3 8 L10.7 4.2 M5.3 8 L10.7 11.8" />,
  ],
  /* Zbior: dwie plyty, ktore na siebie zachodza. Ani jednej krawedzi. */
  agents: [
    <rect key="a" x="2.4" y="2.4" width="7.4" height="7.4" rx="1.8" />,
    <rect key="b" x="6.2" y="6.2" width="7.4" height="7.4" rx="1.8" />,
  ],
  /* Zbior tego, co model wie: dwie plyty w stosie. Dwa glify — iskra Umiejetnosci i stos
   * Pamieci — zeszly sie w jeden 2026-08-31 razem z sekcjami, bo dwie ikony obok siebie mowily
   * „to sa dwie rozne rzeczy" dokladnie tam, gdzie odpowiadaja na jedno pytanie czlowieka.
   * Stos, a nie iskra: to jest ZBIOR, a nie czynnosc, i ta sama gramatyka co Agents. */
  knowledge: [
    <rect key="a" x="2.4" y="3" width="11.2" height="4" rx="1.4" />,
    <rect key="b" x="2.4" y="9" width="11.2" height="4" rx="1.4" />,
  ],
  /* Trzy slupki roznej wysokosci: pomiar, ktory da sie porownac z sasiadem. Ani okregu, ani
   * krawedzi — to nie jest graf i nie obiecuje relacji miedzy kolumnami (niezmiennik 17); ani
   * plyt w stosie, bo zestaw nie jest kolejna polka biblioteki, tylko odczytem z niej. */
  lab: [<path key="l" d="M3.4 12.6 V8.2 M8 12.6 V3.4 M12.6 12.6 V6.2" />],
  /* Zegar pyta cyklicznie. Jeden obrys i wskazowka, bez wezlow ani ozdobnej relacji. */
  triggers: [
    <path key="t" d="M8 2.4 A5.6 5.6 0 1 1 2.4 8 A5.6 5.6 0 0 1 8 2.4 M8 5.1 V8 L10.2 9.4" />,
  ],
  /* Trzy belki roznej dlugosci: nastawy, ktore czlowiek przesuwa. Ani okregu, ani `<line>` —
   * to nie jest zbior i nie jest grafem, wiec nie wolno mu obiecywac ani plyt, ani relacji
   * (niezmiennik 17, ta sama gramatyka co szesc glifow wyzej). */
  settings: [<path key="s" d="M3 4.5 H13 M3 8 H13 M3 11.5 H9" />],
};

/* Trzy glify, ktore nie naleza do zadnej sekcji, wiec nie stoja w mapie wyzej: kladka, lupa
 * i kartka z odcieta kolumna.
 *
 * Kladka nie jest ikona miejsca — jest ZDANIEM o stanie pozycji („tego nie da sie jeszcze
 * uzyc") i stoi obok zdania, ktore mowi czego brakuje. Lupa jest kontrolka. Wrzucone do `PATHS`
 * byly by dwiema pozycjami udajacymi sekcje w mapie, ktorej cala tresc brzmi „jeden glif na
 * sekcje" — a `icon-grammar.test.tsx` liczy te mape z rejestru. */
function Glyph({ children }: { readonly children: readonly ReactElement[] }): ReactElement {
  return (
    <svg
      aria-hidden
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-[13px] shrink-0"
    >
      {children}
    </svg>
  );
}

/** Kladka: pozycja, ktorej nie ma jeszcze czym wypelnic. */
export function LockGlyph(): ReactElement {
  return (
    <Glyph>
      {[
        <rect key="b" x="3.2" y="7" width="9.6" height="6.4" rx="1.6" />,
        <path key="s" d="M5.6 7 V4.8 A2.4 2.4 0 0 1 10.4 4.8 V7" />,
      ]}
    </Glyph>
  );
}

/**
 * Kartka z odcieta lewa kolumna: kontrolka, ktora zwija i rozwija boczne menu.
 *
 * To jest ten sam znak, ktorym zwija panel dom (`../meetnotes`, wiersz „Collapse sidebar" na
 * dole panelu) i kazda aplikacja, w ktorej czlowiek ten gest juz zna. Nie stoi w `PATHS`, bo nie
 * jest ikona MIEJSCA — jest kontrolka, tak samo jak lupa obok; wrzucona tam byla by osma
 * pozycja udajaca sekcje w mapie, ktorej cala tresc brzmi „jeden glif na sekcje".
 *
 * Ani okregu, ani `<line>`: to nie jest ani zbior, ani graf, wiec nie obiecuje relacji, ktorej
 * nie ma (niezmiennik 17). Prostokat i kreska w srodku sa RYSUNKIEM UKLADU, nie zaleznosci.
 */
export function PanelGlyph(): ReactElement {
  return (
    <Glyph>
      {[
        <rect key="b" x="2.2" y="3" width="11.6" height="10" rx="2" />,
        <path key="d" d="M6.4 3 V13" />,
      ]}
    </Glyph>
  );
}

/** Lupa: jedyna droga do szukania, ktora nie jest klawiszem. */
export function SearchGlyph(): ReactElement {
  return (
    <Glyph>
      {[<circle key="o" cx="7.1" cy="7.1" r="4.1" />, <path key="h" d="M10.2 10.2 L13.4 13.4" />]}
    </Glyph>
  );
}

export function NavIcon({
  section,
  big = false,
}: {
  readonly section: string;
  /** Glif w waskiej kolumnie stoi sam, bez etykiety obok, wiec jest o dwa piksele wiekszy. */
  readonly big?: boolean;
}): ReactElement | null {
  const parts = PATHS[section];
  if (parts === undefined) return null;
  return (
    <svg
      aria-hidden
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={big ? 'size-[18px]' : 'size-4'}
    >
      {parts}
    </svg>
  );
}
