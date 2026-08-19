import type { ReactElement } from 'react';

/* Znak Loadouta: najmniejszy PRAWDZIWY graf.
 *
 * Jedno wejscie, dwie rownolegle galezie, jedna synteza — czyli dokladnie dwie z pieciu rzeczy
 * z decyzji D6, ktorych zaden vendor nie zbuduje, bo nie ma w tym interesu: odpalic kilku agentow
 * naraz i zebrac ich wyniki w jeden. Wnetrze aplikacji ma niezmiennik 17 („UI nie rysuje relacji,
 * ktorych nie ma w danych"), wiec marka, ktora JEST najmniejszym grafem prawdziwym, jest jedynym
 * mozliwym ornamentem tego produktu.
 *
 * Do 2026-08-19 znak byl czterema luznymi kwadratami obroconymi o 45 stopni. Cztery luzne
 * kwadraty nie maja krawedzi, wiec nie maja relacji, wiec nie sa grafem — nowy znak doklada
 * dokladnie dwie rzeczy: KRAWEDZIE i KIERUNEK.
 *
 * DWIE WARTOSCI, ANI JEDNEGO LITERALU. Wezly biora `--color-body`, krawedzie `--color-muted`,
 * i oba przez klase, nie przez atrybut `fill`/`stroke` z wartoscia — `checks/quick-tokens.sh`
 * odrzuca kazdy hex w `src/`, a znak jest jedynym miejscem, w ktorym gradient bylby naturalny.
 * Gradientowa wersja mieszka w `docs/branding/`, czyli POZA `src/`, i wlasnie dlatego.
 *
 * W CHROME ZNAK JEST NEUTRALNY: ani akcentu, ani coralu. Akcent znaczy „to jest interaktywne",
 * a coral „to sie dzieje teraz" — a znak wisi w nawigacji takze wtedy, kiedy nic nie chodzi,
 * wiec coral w nim bylby klamstwem (niezmiennik 13, DESIGN §3).
 *
 * KRAWEDZ NIE JEST OBRAMOWANIEM. Do 2026-08-19 brala `--color-line-strong`, czyli biel 16% —
 * wartosc, ktora w tym systemie rysuje wlos na krawedzi szkla. Zmierzone na wyrenderowanej
 * powloce przy 22 px, czyli w jedynym rozmiarze, w jakim znak naprawde stoi w aplikacji: linia
 * 1,25 px w bieli 16% na panelu daje kontrast okolo 1,7 : 1 i po prostu nie czyta sie wcale.
 * Znak skladal sie wtedy z czterech kropek — dokladnie tego, co mial przestac byc, bo cala jego
 * teza brzmi „cztery luzne kwadraty nie maja krawedzi, wiec nie sa grafem". Krawedzie sa TEMATEM
 * tego rysunku, nie chrome wokol niego, wiec biora wartosc z rodziny tekstu. `--color-muted`
 * jest ciemniejsze od `--color-body`, wiec hierarchia zostaje ta sama: wezly nad krawedziami.
 *
 * Geometria jest ta sama, co w `docs/branding/loadout-mark.svg`, i kryterium AC-1 porownuje oba
 * w tym samym biegu testu: rysunek i kod nie moga sie rozjechac. Barwy porownywac nie ma czego:
 * rysunek stoi na `currentColor`, bo jest szablonem do eksportu i barwe wybiera ten, kto go
 * uzywa — dwie wartosci sa decyzja TEJ instancji, tej w nawigacji.
 */

/** Cztery krawedzie: wejscie do dwoch galezi, dwie galezie do syntezy. */
const EDGES = 'M3.7 12 L12 5.1 M3.7 12 L12 18.9 M12 5.1 L20.3 12 M12 18.9 L20.3 12';

/** Cztery wezly. Ostatni jest SYNTEZA i jest wiekszy: z wielu wychodzi jedno. */
const NODES: ReadonlyArray<readonly [number, number, number]> = [
  [3.7, 12, 1.95],
  [12, 5.1, 1.95],
  [12, 18.9, 1.95],
  [20.3, 12, 2.15],
];

export interface MarkProps {
  /** Rozmiar boku w pikselach. Domyslnie 22 — tyle, ile daje mu makieta w nawigacji. */
  readonly size?: number;
}

export function Mark({ size = 22 }: MarkProps): ReactElement {
  return (
    <svg
      aria-hidden
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      className="block shrink-0"
    >
      <path d={EDGES} className="stroke-muted" strokeWidth="1.25" strokeLinecap="round" />
      {NODES.map(([cx, cy, r]) => (
        <circle key={`${String(cx)}-${String(cy)}`} cx={cx} cy={cy} r={r} className="fill-body" />
      ))}
    </svg>
  );
}
