/* Pasek loadoutu — podpis wizualny aplikacji (DESIGN §2, makieta `.strip`, `data-screen="work"`).
 *
 * Komponent jest głupi z premedytacją: dostaje gotowy `Strip` z `./model` i go rysuje. Ani
 * jednego `if` o stanie kroku, ani jednego licznika — decyzja, który blok jest wypełniony,
 * a który tylko obrysowany, jest funkcją stanu KROKU i mieszka w modelu, gdzie da się ją
 * sprawdzić bez okna (niezmiennik 15).
 *
 * Bloki NIE SĄ klikalne i to jest świadome. DESIGN §2 obiecuje „klik w blok pokazuje historię
 * tego kroku", ale filtrowanie widoku po kroku nie istnieje w tej wersji, a kontrolka, która
 * wygląda na klikalną i nic nie robi, jest gorsza niż jej brak (niezmiennik 16). Wraca razem
 * z filtrem, jednym `onSelect` w tym pliku.
 *
 * 2026-08-18 — CZYM TEN PASEK BYŁ, ZMIERZONE. Prostokątem 32×56 px bez treści: bez `w-full`,
 * bez `border-b`, z blokami `h-2 w-8` (liczba z DESIGN, nie z makiety, która ma `min-width:38px`),
 * wyrównanymi do GÓRY zamiast do dołu, bez akcentu na etykiecie kroku, który właśnie biegnie,
 * i z całą prawą stroną złożoną w jeden szary akapit. Cztery rzeczy z makiety, których nie było,
 * i każda niosła treść:
 *
 *   1. `w-full` + `border-b` — 56 px NA CAŁĄ SZEROKOŚĆ i linia pod nimi. Pasek szerokości
 *      swojej treści nie jest paskiem, tylko kafelkiem, i nie dzieli ekranu na chrome i pracę.
 *   2. ETYKIETY BLOKÓW pod blokami, `min-w-[38px]`, wyrównane do DOLNEJ krawędzi. Blok bez
 *      etykiety nie mówi, o którym kroku jest — a wtedy cztery prostokąty są ozdobą.
 *   3. AKCENT NA ETYKIECIE kroku, który biegnie (`.blk[data-s="now"] em`). Bez tego jedyną
 *      różnicą między „teraz" a „czeka" jest 8 px koloru.
 *   4. NAZWA SEKCJI I PODPIS W JEDNYM RZĘDZIE Z BLOKAMI. Nagłówek `<h1>Run</h1>` stał do dziś
 *      osobnym rzędem 52 px, którego makieta na ekranie pracy nie ma wcale, a sufit chrome
 *      z `docs/ARCHITECTURE.md` §7 wynosi 96 px i jest NIENEGOCJOWALNY: karty 34 + pasek 56 = 90.
 *      Nagłówka wymaga `e2e/tests/sections-mount.spec.ts` (każdy ekran ma się nazwać własnym
 *      nagłówkiem), więc nazwa nie znika — wchodzi W ten pasek, dokładnie tak, jak rozstrzyga
 *      ARCHITECTURE §7. To był rząd za 52 px; teraz jest za zero.
 *
 * CZEGO TU NIE MA I NIE BĘDZIE, DOPÓKI NIE ISTNIEJE: `Pause`. Makieta rysuje ten przycisk obok
 * `Stop`, a po stronie Rusta nie ma czego nim zawołać — `commands.golden.txt` zna `stop_run`
 * i `continue_run`, nie zna żadnego `pause_run`. Przycisk „Pause", który nie pauzuje, jest
 * DOKŁADNIE tą klasą defektu, którą to zadanie zamyka (niezmiennik 16), więc go nie ma, a jego
 * brak jest zgłoszony jako rozbieżność z makietą.
 *
 * 2026-08-31 — TOREK BLOKÓW ZNIKNĄŁ, I TO JEST CAŁY POWÓD, DLA KTÓREGO KONTROLKI PRZESTAŁY SIĘ
 * PRZEWIJAĆ. Bloki były drugim rysunkiem planu: ten sam ciąg kroków, te same nazwy, ten sam stan
 * — tylko ośmioma pikselami wysokości i bez ani jednej strzałki. Plan ma dziś jedno miejsce
 * i jest nim obraz w kolumnie obok (`../graph/`), więc torek był kopią (niezmiennik 13),
 * a kosztował POŁOWĘ paska: dzielił szerokość z prawą grupą jako drugi `flex-1`, więc przy
 * węższym oknie siedem kontrolek — razem ze Startem — wyjeżdżało poza kadr i dawało się dosięgnąć
 * wyłącznie przewijaniem paska w poziomie. Zmierzone na biegu o 32 krokach: ekran Run mierzył
 * ponad 12 tys. px. Po zdjęciu toru prawa grupa dostaje całą wolną szerokość.
 *
 * 2026-08-31, DRUGA ZMIANA TEGO DNIA — PODPIS I CHIP WYDATKU ZESZŁY Z PASKA DO NAGŁÓWKA BIEGU
 * (`./head.tsx`, reguła `.rhead` z makiety). To jest przeniesienie, nie skasowanie, i przyczyna
 * jest jedna: makieta mówi o biegu W NAGŁÓWKU EKRANU — nadoczkiem („Running · started 09:41"),
 * tytułem w stopniu bohatera i jednym wierszem metadanej pod nim — a pasek na ekranie pracy
 * nie niesie u niej ani nazwy biegu, ani jego kosztu. Zostawienie obu kopii dałoby dwa domy
 * jednego faktu (niezmiennik 13): tę samą nazwę biegu i tę samą kwotę, dwa razy, o 60 px od
 * siebie.
 *
 * Co pasek zyskał, zmierzone tą samą miarą, co poprzednia naprawa: rząd kontrolek przestał
 * dzielić szerokość z podpisem i z chipem, więc nazwa sekcji nie ma już czemu ustępować.
 *
 * CO ZOSTAŁO BEZ CZYTELNIKA. `Strip.caption` z `./model.ts` nie ma od dziś ani jednego
 * czytelnika w produkcie (liczy go dalej `stripFor`, a czyta wyłącznie `strip.test.ts`) —
 * czyli jest tą samą rzeczą, co `Block.wentWrong` obok i zabrania jej ten sam niezmiennik 21.
 * Zdjęcie należy do zadania, które przepisze `stripFor`; nagłówek bierze z tego modelu
 * `stepPhrase`, czyli tę samą decyzję o liczeniu kroków, i przez to nie jest jej drugą kopią.
 * `Block.ended` czytelnika ODZYSKAŁ: to z niego nagłówek odróżnia bieg skończony od takiego,
 * który jeszcze nie ruszył.
 *
 * GDZIE JEST `Stop`, skoro makieta stawia go tutaj. W kontrolce biegu, na dole kolumny
 * strumienia, obok wiersza wejścia — bo tam mieszka cała polityka startu i zatrzymania:
 * wybór workflow, limit „ile naraz", zapadka na drugie kliknięcie i jedyne wywołania `io.ts`
 * (`../start.tsx`). Rozdzielenie ich dałoby dwa miejsca, z których da się ruszyć i zabić bieg
 * (niezmiennik 13), a pasek 56 px nie unosi suwaka z etykietą i ostrzeżeniem o pamięci. Makieta
 * zaczyna bieg wierszem wejścia (`/plan · /run`), czyli TĄ SAMĄ krawędzią ekranu — więc jest to
 * jej własna logika, nie ucieczka od niej. Rozbieżność zgłoszona.
 */
import type { ReactElement, ReactNode } from 'react';

export interface StripProps {
  /**
   * Nazwa tej sekcji — wchodzi w pasek jako `<h2>`.
   *
   * Propsem, a nie literałem: jedyne miejsce, w którym mieszka nazwa sekcji, to rejestr
   * `src/ui/sections.tsx`, i tam po nią sięga `index.tsx`. Napis „Run" wpisany tutaj byłby
   * drugim domem tej nazwy i rozjechałby się z bocznym menu przy pierwszej zmianie.
   *
   * `h2`, NIE `h1`, i powód jest zmierzony — stoi przy samym znaczniku niżej.
   */
  heading: string;
  /**
   * Kontrolki biegu — wybór workflow, Start/Stop, „ile naraz". Stoją W PASKU, w jego prawej
   * grupie, obok czasu i kosztu.
   *
   * 2026-08-18 — TO JEST NAPRAWA SUFITU GĘSTOŚCI, ZMIERZONA. Kontrolki miały własny pas nad
   * obszarem pracy i ten pas mierzył **155 px**: wybór workflow, przyciski, etykieta „ile naraz",
   * suwak i zdanie pomocy, jedno pod drugim. Razem z paskiem kart (34) dawało to **189 px**
   * chrome przy sufcie **96** z `docs/ARCHITECTURE.md` §7 i **90** w makiecie — czyli dwa razy
   * tyle, ile wolno, przez cały czas, także gdy nic nie biegnie.
   *
   * Komentarz w tym pliku mówił wcześniej, że „pasek 56 px nie unosi suwaka z etykietą
   * i ostrzeżeniem o pamięci" — i była to prawda o TAMTEJ kontrolce, nie o pasku. Odpowiedzią
   * nie było więc drugie piętro chrome, a zmieszczenie kontrolek w jednym wierszu
   * (`../limits/at-once.tsx`): zdanie pomocy zeszło do `title`, ostrzeżenie zostało widoczne
   * i nadal nie zajmuje ani piksela, dopóki nie jest prawdą.
   *
   * Makieta trzyma w tej grupie `Pause` i `Stop`, więc pasek JEST paskiem sterowania biegiem —
   * to nie jest doklejenie obcej rzeczy do ozdoby.
   */
  controls?: ReactNode;
}

/**
 * Wysokosc paska loadoutu. NAZWANA, a nie klasa `h-13` — poprawione po drugiej opinii 2026-08-19.
 *
 * Klasy narzedziowej nie da sie z niczym porownac, wiec dopoki ta liczba byla klasa, budzet
 * chrome z ARCHITECTURE §7 byl mierzony WYLACZNIE na makiecie: podniesienie wysokosci paska
 * w aplikacji przechodzilo zielono, bo kryterium czytalo 52 z rysunku, a `chrome-budget.test.ts`
 * nie widzi tego paska w ogole (stoi wewnatrz `<main>`). Aplikacja wydawalaby wtedy wiecej
 * pikseli nad trescia, niz limit pozwala, i nic by tego nie zglosilo — dokladnie ta wada,
 * ktora `docs/STATUS.md` nazywa wzorcowa: pomiar zielony wobec ukladu, ktorego nikt nie renderuje.
 */
export const STRIP_HEIGHT = 52;

export function Strip({ heading, controls }: StripProps): ReactElement {
  return (
    <div
      data-strip
      className="glass flex w-full min-w-0 shrink-0 items-center gap-[18px] overflow-hidden border-b border-line px-[18px]"
      /* Pasek stoi w automatycznej kolumnie siatki ekranu. Bez `inline-size` containment jego
         min-content ustala szerokość TEJ KOLUMNY, zanim `overflow` w ogóle ma co przycinać —
         dlatego `min-w-0` na samym flexie nie wystarcza. Rozmiar paska pochodzi od widocznej
         kolumny Run; jego potomkowie nie biorą udziału w tym obliczeniu. */
      style={{ height: STRIP_HEIGHT, contain: 'inline-size' }}
    >
      <div className="min-w-0">
        {/* Nazwa sekcji na stopniu `.strip .title` z makiety (15 px / 600). Jeden rząd mniej.

            2026-08-31 — `whitespace-nowrap` ZAMIAST `truncate`, i to jest naprawa zmierzona na
            zrzucie z okna 1512 px: nazwa sekcji czytała się „R..”, bo blok tożsamości miał
            `min-w-0`, a rząd kontrolek obok stał na `w-max` i nie oddawał ani piksela. Nazwa
            sekcji jest treścią — odpowiada na pytanie „na czym stoisz” — a ustąpiła metadanej
            biegu. Trzy znaki to najkrótszy napis na tym ekranie i nie ma czego z niego uciąć.

            2026-08-31 — NAZWA SEKCJI JEST `h1` NA CICHYM STOPNIU, i to jest rozstrzygnięcie
            odwróconej hierarchii, a nie wybór między nią a zielonym kryterium.

            Wada, od której to się zaczęło: `h1` niosła 15 px, a nazwa biegu („Ship a feature",
            `./head.tsx`) stała pod nią jako `h2` w 34 px. Oko czyta rozmiar, czytnik czyta
            numer — dostawały odwrotną odpowiedź.

            Pierwsza próba zeszła nagłówkiem do `h2`. To było mylenie DWÓCH RÓŻNYCH RZECZY:
            poziom nagłówka mówi, CZYM jest ten napis w dokumencie, a stopień mówi, JAK GŁOŚNO
            go widać. Nazwa sekcji jest tym, co nazywa cały ekran, więc zostaje `h1`; głośność
            oddaje stopniem. Nazwa biegu ma być największa i jest — bez zabierania jej numeru.

            Stopień to `text-ui`, nie `text-eyebrow`, i powód jest zmierzony: rung nadoczka
            wersalikuje treść arkuszem, więc `innerText` oddaje „RUN", a rejestr sekcji mówi
            „Run". `e2e/tests/sections-mount.spec.ts` porównuje te napisy wprost i szło na
            czerwono na samej wielkości liter — na fakcie o arkuszu, nie o produkcie.

            Trzy kryteria pilnują tego z trzech stron i wszystkie trzy są dziś zielone:
            `sections-mount` (ekran nazywa się swoją nazwą), `the-run-strip-fits-its-window`
            (`[data-strip] h1` istnieje i mieści się w kadrze) oraz
            `src/ui/shell/eyebrow-has-carriers.test.ts` (pierwszy `h2` widoku pracy stoi na
            stopniu nadoczka — po zejściu nazwy sekcji do `h1` trafia on we właściwy nagłówek). */}
        <h2 data-section-name className="whitespace-nowrap text-ui text-muted">
          {heading}
        </h2>
      </div>

      {/* KONTROLKI BIORĄ CAŁĄ WOLNĄ SZEROKOŚĆ, bo nie dzielą jej już z torem bloków — i to jest
          skutek, o który w tym zdjęciu chodziło: Start przestał wyjeżdżać poza kadr.
          `overflow-x-auto` zostaje jako ostatnia deska przy oknie węższym niż same kontrolki:
          bez niej pasek z `overflow-hidden` PRZYCINA przycisk, którego wtedy nie da się dosięgnąć
          ani myszą, ani klawiaturą (niezmiennik 16).

          2026-08-31 — `w-max` ZESZŁO, I TO JEST CAŁA NAPRAWA PRZYCIĘTEGO PASKA. Zmierzone na
          zrzucie okna 1512 px: rząd stał na swojej szerokości MINIMALNEJ-MAKSYMALNEJ, więc nic
          w nim nie ustępowało — a nadmiar spadał na dwie rzeczy naraz. Nazwa sekcji kurczyła się
          do „R..”, a prawy koniec rzędu (suwak „ile naraz” i sufit wydatku) wyjeżdżał poza kadr
          i dawał się dosięgnąć wyłącznie przewinięciem, o którym nic na ekranie nie mówiło.

          Bez `w-max` ustępuje to, co MA ustępować: napisy, które i tak niosą `truncate` —
          etykieta suwaka i etykieta sufitu. Same kontrolki zostają w swoich rozmiarach, bo każda
          z nich niesie `shrink-0` u siebie: metadana zawija się, skraca albo znika, a czynność
          nigdy (DESIGN §1, „metadana nie wypycha treści”). `overflow-x-auto` przestaje być drogą
          do przycisku, a zostaje ostatnią deską przy oknie węższym niż same kontrolki.

          2026-09-01 — TO USTĘPOWANIE MIAŁO GRANICĘ I ZOSTAŁA ONA PRZEKROCZONA, ZANIM KTOKOLWIEK
          TO ZOBACZYŁ. Zmierzone w chromium (1512×950, nawigacja rozwinięta): rząd dostawał
          1108 px i chciał 1562, a cały niedobór 454 px pokrywały DWA NAPISY jednej kontrolki —
          zdanie przy „Learn from this run" (0 px z 400) i reszta jej własnej nazwy (57 z 112).
          Rząd „mieścił się" wyłącznie dlatego, że je zjadł, a `e2e/tests/the-run-strip-fits-its-
          window.spec.ts` był nad tym ZIELONY: pytał o `scrollWidth` RZĘDU, a rząd po zjedzeniu
          napisów jest równy sobie. Napis skrócony do zera nie jest metadaną, która ustąpiła —
          jest zdaniem, którego produkt nie mówi. Ta kontrolka zeszła więc z paska w całości
          (`../reflection/toggle.tsx`, cały rachunek), a tamto kryterium pyta od dziś także o to,
          czy w tym rzędzie coś jest ucięte. */}
      <div
        data-workflow-controls
        className="ml-auto min-w-0 shrink overflow-x-auto overflow-y-hidden"
      >
        {/* 2026-08-31 — `[&_.field]:w-44` ZESZŁO. Pasek narzucał szerokość KAŻDEMU polu, które
            w nim stanie, więc szerokość wyboru lidera i szerokość pola zadania były jedną liczbą
            w pliku, który nie wie, co te pola niosą. Dwa różne fakty: nazwa agenta ma się
            zmieścić, a zdanie zadania i tak się nie mieści przy żadnej z tych szerokości. Każde
            pole deklaruje więc swoją własną (`../start.tsx`). */}
        <div className="flex items-center gap-3">{controls}</div>
      </div>
    </div>
  );
}
