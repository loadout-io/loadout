import type { ReactElement } from 'react';

/* Glify nawigacji. Siedem, po jednym na sekcje, 16 px, obrys, `currentColor`.
 *
 * GRAMATYKA JEST TRESCIA, NIE STYLEM, i jest sprawdzana (T-46 AC-5):
 *
 *   wezly i krawedzie  ->  rzecz, ktora JEST grafem            (Workflows)
 *   plyty w stosie     ->  rzecz, ktora jest ZBIOREM           (Agents, Skills, Memory)
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
  /* Zbior zdolnosci: iskra o czterech ramionach. */
  skills: [
    <path key="s" d="M8 2.2 L9.5 6.5 L13.8 8 L9.5 9.5 L8 13.8 L6.5 9.5 L2.2 8 L6.5 6.5 Z" />,
  ],
  /* Zbior zapisow: dwie plyty w stosie. */
  memory: [
    <rect key="a" x="2.4" y="3" width="11.2" height="4" rx="1.4" />,
    <rect key="b" x="2.4" y="9" width="11.2" height="4" rx="1.4" />,
  ],
  /* Zegar pyta cyklicznie. Jeden obrys i wskazowka, bez wezlow ani ozdobnej relacji. */
  triggers: [
    <path key="t" d="M8 2.4 A5.6 5.6 0 1 1 2.4 8 A5.6 5.6 0 0 1 8 2.4 M8 5.1 V8 L10.2 9.4" />,
  ],
  /* Trzy belki roznej dlugosci: nastawy, ktore czlowiek przesuwa. Ani okregu, ani `<line>` —
   * to nie jest zbior i nie jest grafem, wiec nie wolno mu obiecywac ani plyt, ani relacji
   * (niezmiennik 17, ta sama gramatyka co szesc glifow wyzej). */
  settings: [<path key="s" d="M3 4.5 H13 M3 8 H13 M3 11.5 H9" />],
};

export function NavIcon({ section }: { readonly section: string }): ReactElement | null {
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
      className="size-4"
    >
      {parts}
    </svg>
  );
}
