/* Kroki biegu jako JEDNA ŚCIEŻKA W PIONIE — znacznik przy każdym kroku i linia, która je łączy.
 *
 * CO TO NAPRAWIA, ZMIERZONE 2026-08-31 (zgłoszenie właściciela: „UX totalnie nieoczywisty").
 * Kolumna planu stawiała kroki jeden pod drugim, każdy w osobnej karcie, i ANI JEDEN piksel nie
 * mówił, że są ciągiem. Cztery karty to stos: nie widać z nich ani ile kroków zostało, ani które
 * są już za tobą — obie te rzeczy człowiek musiał policzyć sam, za każdym razem, patrząc na
 * ekran, na którym pracuje czterech agentów naraz.
 *
 * ODPOWIEDŹ JEST Z MAKIETY, nie wymyślona: `docs/mockup/index.html`, reguły `.rail`, `.step`,
 * `.pip` i `.step::before`. Znacznik niesie STAN kroku (ptaszek / kropka / glif / numer),
 * a linia między znacznikami niesie POSTĘP: ciągła za krokiem, który się udał, przerywana przed
 * tym, który jeszcze nie ruszył, i żadna po ostatnim.
 *
 * NUMER JEST POZIOMEM W GRAFIE, NIE MIEJSCEM W TABLICY — poprawka z 2026-08-31, zgłoszenie
 * właściciela: „w sumie te numerki kłamią bo kilka może iść na raz cnie". Do tego dnia stało tu
 * `at + 1`, więc dwa kroki wiszące na tym samym poprzedniku dostawały „2" i „3" i kolumna
 * obiecywała porządek CAŁKOWITY nad danymi, które znają tylko CZĘŚCIOWY. Liczy to dziś
 * `levelsOf` (`./model.ts`) i tam stoi całe uzasadnienie. Na ekranie widać to dwiema rzeczami
 * naraz: kroki jednego poziomu mają TEN SAM numer, a między nimi nie ma linii — bo linia znaczy
 * „potem", a tego między nimi nie ma (niezmiennik 17). Makieta rysuje ten przypadek
 * (`.step[data-at-once]`), więc jest on wyrocznią, a nie wymysłem tego pliku.
 *
 * DLACZEGO LINIA JEST KLASĄ NA ELEMENCIE, A NIE REGUŁĄ `::before` W ARKUSZU. Reguła w pliku
 * `.css` da się sprawdzić wyłącznie odczytem tego pliku, a taki odczyt przechodzi także wtedy,
 * gdy żaden element jej nie nosi — czyli dokładnie wtedy, gdy na ekranie nie ma nic
 * (niezmiennik 29). Element z klasą jedzie do przeglądarki razem z krokiem i widać go w tym
 * samym markupie, w którym widać kartę.
 *
 * DLACZEGO ZNACZNIK STOI W MARKUPIE ZA KARTĄ, A NA EKRANIE PRZED NIĄ. Miejsce na ekranie
 * rozstrzyga siatka (`gridColumn`), więc kolejność w drzewie jest wolna — i jest wydana na dwie
 * rzeczy. Pierwsza: czytnik ekranu dostaje najpierw TREŚĆ kroku (nazwa, stan słowem, kto go
 * robi), a znacznik jest `aria-hidden`, bo powtarza to samo kształtem. Druga jest mechaniczna:
 * cudze kryteria tną markup listy na kafelki po `data-step="` (`./each-state-draws-its-own-step
 * .test.tsx`, `../the-plan-reaches-the-screen.test.tsx`), więc znacznik postawiony PRZED kartą
 * wpadałby do wycinka kroku POPRZEDNIEGO i punkt „skończony krok nie ma glifu porażki" sądziłby
 * sąsiada.
 *
 * WIERSZE SIATKI SĄ JAWNE, nie zostawione automatowi. Pod kartą kroku, który o coś pyta, stoi
 * czasem karta pytania — czyli wiersz, którego pozostałe kroki nie mają. Automat rozkładający
 * elementy po kolei zostawiłby wtedy w kolumnie znaczników dziurę i linia przestałaby dochodzić
 * do następnego znacznika. Znacznik ROZPINA SIĘ więc od swojego wiersza do wiersza kroku
 * następnego i dzięki temu linia zawsze kończy się dokładnie pod nim.
 */
import type { CSSProperties, ReactElement, Ref } from 'react';
import { Fragment } from 'react';
import type { GraphStep, Plan } from './model';
import { RunTile } from './tile';

/** Odstęp między krokami — 14 px z reguły `.step` w makiecie (`padding:0 0 14px`). */
const STEP_GAP = 8;

/** Siatka ścieżki: kolumna znaczników i kolumna kart. */
/* JEDNA KOLUMNA. Rynna znacznikow zeszla 2026-09-01 na zgloszenie wlasciciela („calkowiecie to
 * wywal"): po zdjeciu numerow niosla kropke i kreske, czyli KSZTALTEM to, co karta obok mowi
 * SLOWEM — stan chipem, a zaleznosc zdaniem „after <krok>", po IMIENIU, a nie po polozeniu.
 * Dwa nosniki na jeden fakt (niezmiennik 13), z ktorych jeden zabieral 27 px szerokosci przy
 * kazdym z dziesieciu krokow. */
const PATH: CSSProperties = { rowGap: STEP_GAP };

/**
 * Miejsce karty kroku: druga kolumna, wiersz zostawiony siatce.
 *
 * STAŁA, i to jest wymuszone, nie oszczędne. Cudze kryterium porównuje markup dwóch kart CO DO
 * ZNAKU, żeby udowodnić, że dwa stany kroku o jednym znaczeniu wyglądają tak samo
 * (`../a-broken-step-does-not-look-like-a-waiting-one.test.tsx`) — numer wiersza wpisany w styl
 * karty robiłby z każdej karty inny napis i to porównanie nie miałoby jak przejść nigdy.
 *
 * Wiersz i tak wychodzi ten sam. Znacznik ma wiersz JAWNY (musi rozpiąć się do wiersza kroku
 * następnego, żeby linia miała gdzie się skończyć), a znaczniki stoją w kolumnie pierwszej;
 * karty są więc jedynym, co siatka układa w kolumnie drugiej po kolei, i lądują dokładnie
 * w tych wierszach, które `placed` policzył dla znaczników.
 */
const CARD_CELL: CSSProperties = {};

export interface StepPathProps {
  plan: Plan;
  /** Wejście w tego, kto ten krok robi — albo brak, kiedy wołający nie ma dokąd wpuścić. */
  onOpen?: (stepId: string) => void;
  /**
   * Karta pytania pod krokiem, który je zadał — albo `null`, kiedy ten krok o nic nie pyta.
   *
   * Składa ją wołający (`./graph.tsx`), bo to on wie, czy odpowiedź ma dokąd pojechać; ścieżka
   * wie tylko, w którym wierszu ta karta stoi. `null` jest tu odpowiedzią, nie brakiem jej:
   * bez niego ścieżka rezerwowałaby wiersz pod każdym krokiem i odstępy między krokami
   * przestałyby być równe.
   */
  asking: (step: GraphStep) => ReactElement | null;
  /**
   * Co stoi POD ostatnim krokiem, w tym samym przewijanym wycinku.
   *
   * Zdanie o końcu biegu należy do ścieżki, nie do dna kolumny: przypięte na dole stało od
   * ostatniego kroku o pół ekranu pustki i czytało się jak stopka, a nie jak następny punkt tej
   * samej drogi (zmierzone na zrzucie okna 1512×950). Wchodzi TĄ drogą, a nie własnym montażem
   * niżej, bo dokładnie jedno miejsce w drzewie ma je stawiać (niezmiennik 13).
   */
  tail?: ReactElement | null;
  /** Uchwyt do listy: krok, który pracuje, ma zostać przewinięty na widok. */
  listRef: Ref<HTMLDivElement>;
}

export function StepPath({ plan, onOpen, asking, tail, listRef }: StepPathProps): ReactElement {
  return (
    <div ref={listRef} data-step-list className="grid content-start overflow-auto p-2" style={PATH}>
      {plan.steps.map((step) => (
        <Fragment key={step.id}>
          {/* KARTA. Ta sama, co na płótnie — jeden kafelek kroku w całym repo. */}
          <RunTile
            step={step}
            plan={plan}
            style={CARD_CELL}
            {...(onOpen === undefined || step.who === undefined
              ? {}
              : {
                  onOpen: () => {
                    onOpen(step.id);
                  },
                })}
          />

          {/* KARTA PYTANIA POD SWOIM KROKIEM. Wraca tu po zdjeciu rynny znacznikow: wisiala
              w tej samej siatce i zeszla razem z nia. Siatka ma dzis JEDNA kolumne, wiec karta
              nie potrzebuje ani numeru wiersza, ani numeru kolumny — stoi za kafelkiem,
              ktorego dotyczy. */}
          <AskedHere>{asking(step)}</AskedHere>
        </Fragment>
      ))}
      {/* POD OSTATNIM KROKIEM, przez całą szerokość ścieżki: to jest kolejny punkt tej samej
          drogi, a nie przypis do niej. Wiersz zostawiamy siatce — stoi na końcu, więc trafia
          pod ostatnie, co siatka postawiła w kolumnie drugiej. */}
      {tail === undefined || tail === null ? null : (
        <div style={{ gridColumn: '1 / -1' }}>{tail}</div>
      )}
    </div>
  );
}

/**
 * Karta pytania w swoim wierszu siatki — albo nic.
 *
 * Osobny komponent, bo wiersz wolno zająć DOPIERO wtedy, gdy jest co w nim postawić: pusty
 * pojemnik dalej zajmuje wiersz razem z odstępem, więc odstępy między krokami rozjeżdżałyby
 * się o 14 px przy każdym kroku, który akurat o nic nie pyta.
 *
 * `nodrag`/`nopan` są nazwami zachowania React Flow i jadą tu razem z kartą, bo ta sama karta
 * stoi też na płótnie — bez nich pociągnięcie w polu odpowiedzi przesuwa widok zamiast
 * zaznaczać tekst.
 */
function AskedHere({ children }: { children: ReactElement | null }): ReactElement | null {
  if (children === null) return null;
  return <div className="nodrag nopan">{children}</div>;
}
