/* Obraz biegu: kroki jako JEDNA PIONOWA ŚCIEŻKA. To jest jedyna droga tego ekranu.
 *
 * 2026-08-31 — PŁÓTNO ZESZŁO Z EKRANU BIEGU, i to nie jest przestawienie mebli. Do dziś stał
 * tutaj warunek `if (!view.drawable) return <StepList …/>`: ścieżka rysowała się WYŁĄCZNIE dla
 * planu, który nie niesie zapisanych pozycji. Każdy prawdziwy workflow pozycje ma — zapisuje je
 * edytor przy każdym kafelku — więc w produkcie zawsze wypadało płótno React Flow. Zmierzone na
 * zrzucie okna 1512×950: kafelki wysokie na 40 px, nieczytelne, a karta pytania szeroka na
 * ~120 px, czyli za wąska na zdanie, którego dotyczy. Kryterium ścieżki było przy tym ZIELONE,
 * bo fikstura podawała plan bez układu — mechanizm wylądował i nikt go nie zamontował, czyli
 * dokładnie ta klasa wady, dla której to repo powstało (niezmiennik 29).
 *
 * PŁÓTNO NALEŻY DO EDYTORA (`../../workflows/canvas/`) i tam zostaje w całości. Tam człowiek
 * układa kafelki i ciągnie strzałki, więc tam pozycja z pliku JEST treścią. Bieg odpowiada na
 * inne pytanie — „na czym ta praca stoi" — a odpowiedź na nie jest jednowymiarowa: kolejność.
 * Wyrocznią jest makieta (`docs/mockup/index.html`): `.work` daje kolumnie kroków 376 px
 * i stawia ją PIERWSZĄ, `.step` dzieli ją na rynnę 30 px i kartę, a na całym ekranie biegu nie
 * ma ani jednego płótna.
 *
 * CO ZNIKŁO RAZEM Z PŁÓTNEM: `CanvasTile` z kotwicami `<Handle>`, stała `nodeTypes`, grot,
 * sufit powiększenia, plakietka biblioteki, `onNodeClick` naprawiające martwy środek kafelka
 * oraz `nodesFor`/`edgesFor` — jedyni wołający `tilesOf`/`arrowsOf`. Import `@xyflow/react`
 * i oba arkusze biblioteki zeszły z tej sekcji; edytor importuje je u siebie i nic tam nie
 * ruszamy.
 *
 * TEN PLIK JEST MONTAŻEM, NIE LOGIKĄ. Kształt ścieżki — znacznik, linia, wiersze siatki —
 * mieszka w `./path.tsx`, karta kroku w `./tile.tsx`, a to, co mówi jej ostatnia linia,
 * w `./model.ts`. Tam też jest sądzone.
 *
 * CZEGO TU NIE MA: ANI JEDNEJ KONTROLKI ZMIENIAJĄCEJ PLAN. Bieg nie jest miejscem, w którym
 * zmienia się plan (niezmiennik 16). Kliknięcie w kartę OTWIERA to, co ten krok powiedział,
 * i nic poza tym.
 */
import './graph.css';
import type { ReactElement } from 'react';
import { useEffect, useRef } from 'react';
import { Asked } from '../feed/feed';
import type { GraphStep, Plan } from './model';
import { StepPath } from './path';

/**
 * Karta pytania tego kroku — albo `null`, kiedy ten krok o nic nie pyta ALBO nie ma dokąd
 * oddać odpowiedzi.
 *
 * Funkcja, a nie komponent, bo ścieżka kroków musi ODRÓŻNIĆ „nie ma karty" od „karta jest":
 * wiersz siatki wolno zająć dopiero wtedy, gdy jest co w nim postawić, a komponent renderujący
 * `null` jest elementem tak samo jak ten, który coś rysuje. Karta i droga odpowiedzi są te same,
 * co pod strumieniem — dwa komplety przycisków na jedno pytanie to dwa miejsca, z których da
 * się puścić bieg (niezmiennik 13).
 */
function askedUnder(
  step: GraphStep,
  answer: ((questionId: number, option: string) => void) | null,
): ReactElement | null {
  if (step.asked === undefined || answer === null) return null;
  return <Asked question={step.asked} onAnswer={answer} />;
}

export interface RunGraphProps {
  plan: Plan;
  /**
   * Co robi kliknięcie w kartę kroku — albo nic, kiedy wołający nie ma dokąd wpuścić.
   *
   * DRZWI SĄ DOKŁADNIE TAM, GDZIE WIEMY, KTO ZA NIMI STOI: karta dostaje przycisk wtedy
   * i tylko wtedy, gdy krok niesie `who`. Krok, o którym strumień jeszcze nic nie powiedział,
   * nie ma agenta do pokazania, a przycisk otwierający pusty ekran jest kontrolką bez skutku
   * z dodatkowym krokiem (niezmiennik 16).
   *
   * 2026-08-31: prawa kolumna ekranu pracy zniknęła, a była jedyną drogą do ekranu jednego
   * agenta. Bez tego propsa `openAgent`, `session/` i `rerun_step` zostają mechanizmem bez
   * ani jednego produkcyjnego wołającego (niezmiennik 16).
   */
  onOpen?: (stepId: string) => void;
  /**
   * Odpowiedź człowieka na pytanie stojące przy kroku — albo brak, i wtedy karty tu nie ma.
   *
   * TA SAMA DROGA, CO Z DOŁU STRUMIENIA: wołający podaje tę samą funkcję, którą podaje
   * komponentowi strumienia (`../index.tsx`, `answerQuestion`), więc odpowiedź jedzie jednym
   * torem niezależnie od tego, w którym z dwóch miejsc karta akurat stoi. Druga droga do
   * odblokowania biegu jest dokładnie tym, co rozjeżdża się po cichu (niezmiennik 13).
   *
   * Brak propsa znaczy „ten rysunek nie umie przyjąć odpowiedzi", a wtedy karty nie ma wcale:
   * karta z przyciskami, które nic nie robią, jest gorsza od jej braku (niezmiennik 16).
   */
  onAnswer?: (questionId: number, option: string) => void;
  /**
   * Co stoi POD ostatnim krokiem, w tej samej kolumnie — dziś zdanie o tym, co się stanie,
   * kiedy ostatni krok zzielenieje (`./after-run.tsx`).
   *
   * Jedzie TĘDY, a nie osobnym montażem w ekranie, bo ma stać pod OSTATNIM KROKIEM, a nie na
   * dnie kolumny — a gdzie kończą się kroki, wie ten rysunek, nie ekran. Ekran montowałby je
   * pod obszarem o wysokości całej kolumny i zdanie stało od ostatniego kroku o pół ekranu
   * pustki (zmierzone 2026-08-31 na zrzucie okna 1512×950).
   */
  footer?: ReactElement | null;
}

/**
 * Ścieżka kroków tego biegu — zawsze, niezależnie od tego, co plan wie o swoim kształcie.
 *
 * KOLEJNOŚĆ JEST TU CAŁĄ RELACJĄ, i to jest uczciwe wobec reguły 17: kroki stoją w kolejności
 * planu, a to, co po czym idzie, mówi SŁOWAMI ostatnia linia karty („after Reproduce",
 * `measureOf` w `./model.ts`) — czyli zdaniem wyliczonym ze strzałek z pliku, a nie krzywą
 * między dwoma punktami. Nic tu nie jest zgadnięte i nic nie jest ozdobą.
 */
export function RunGraph({ plan, onOpen, onAnswer, footer }: RunGraphProps): ReactElement {
  /* KROK, KTÓRY PRACUJE, MA BYĆ WIDOCZNY, a przy trzydziestu kilku krokach nie jest.
   *
   * Ścieżka przewija się we własnym wycinku, więc krok, który właśnie idzie, potrafi stać poza
   * nim — a to jest dokładnie ta jedna rzecz, po którą człowiek na tę kolumnę patrzy. Zgłoszenie
   * właściciela 2026-08-23: „nie wiadomo które jak chodzą w sumie". Ta sama linia stała do
   * 2026-08-31 w pasku loadoutu i zeszła razem z jego torem bloków.
   *
   * BEZ RUCHU: `behavior` zostaje domyślne, czyli natychmiastowe. Sufit z ARCHITECTURE §7 to DWA
   * animujące się regiony na całą aplikację, a płynne przewijanie byłoby trzecim.
   *
   * `block: 'nearest'` pilnuje, żeby ruszyła TYLKO ta kolumna: bez tego przeglądarka ma prawo
   * poruszyć także stroną, a kolumna stoi u jej krawędzi i nie ma dokąd jechać.
   *
   * PIERWSZY pracujący, kiedy pracuje ich kilku. Bieg równoległy jest zwykłym biegiem
   * (niezmiennik 11), a wybieranie „ważniejszego" z trzech byłoby relacją, której w danych nie
   * ma (niezmiennik 17). */
  const list = useRef<HTMLDivElement>(null);
  /* KLUCZ, NIE POZYCJA W LIŚCIE, i to jest naprawa z 2026-08-31, nie kosmetyka: między kartami
   * stoi czasem karta pytania, więc n-te dziecko przestało być n-tym krokiem. Wersja licząca po
   * indeksie przewijała wtedy do SĄSIADA i nie było tego po czym poznać — kolumna dalej się
   * przewijała, tylko o jeden krok za daleko. */
  const working = plan.steps.find((step) => step.status === 'working')?.id ?? '';
  useEffect(() => {
    if (working === '') return;
    list.current
      ?.querySelector(`[data-step="${working}"]`)
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [working]);

  return (
    <StepPath
      plan={plan}
      listRef={list}
      {...(onOpen === undefined ? {} : { onOpen })}
      {...(footer === undefined ? {} : { tail: footer })}
      asking={(step) => askedUnder(step, onAnswer ?? null)}
    />
  );
}
