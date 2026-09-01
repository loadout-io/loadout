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
 * PODPIS ZOSTAJE i dalej liczy się z KROKÓW („Fix the CSV parser · step 3 of 4"). To jest zdanie
 * o biegu, a nie drugi jego rysunek: obraz obok pokazuje kształt, podpis mówi, na czym stoimy.
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
import type { Strip as StripModel } from './model';

export interface StripProps {
  strip: StripModel;
  /**
   * Nazwa tej sekcji — wchodzi w pasek jako `<h1>`.
   *
   * Propsem, a nie literałem: jedyne miejsce, w którym mieszka nazwa sekcji, to rejestr
   * `src/ui/sections.tsx`, i tam po nią sięga `index.tsx`. Napis „Run" wpisany tutaj byłby
   * drugim domem tej nazwy i rozjechałby się z bocznym menu przy pierwszej zmianie.
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

export function Strip({ strip, heading, controls }: StripProps): ReactElement {
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
        {/* Nazwa sekcji na stopniu `.strip .title` z makiety (15 px / 600). Jeden rząd mniej. */}
        <h1 className="truncate text-heading text-ink">{heading}</h1>
        {/* Podpis, i tylko tutaj. Numer kroku żyje WYŁĄCZNIE na tym pasku (niezmiennik 13):
            nagłówek z własnym licznikiem i linia `done` z własnym czasem to trzy żywe regiony
            na jeden fakt przy limicie 1. */}
        {strip.caption === '' ? null : <p className="value truncate">{strip.caption}</p>}
      </div>

      {/* KONTROLKI BIORĄ CAŁĄ WOLNĄ SZEROKOŚĆ, bo nie dzielą jej już z torem bloków — i to jest
          skutek, o który w tym zdjęciu chodziło: Start przestał wyjeżdżać poza kadr.
          `overflow-x-auto` zostaje jako ostatnia deska przy oknie węższym niż same kontrolki:
          bez niej pasek z `overflow-hidden` PRZYCINA przycisk, którego wtedy nie da się dosięgnąć
          ani myszą, ani klawiaturą (niezmiennik 16). `w-max` zachowuje prawdziwe rozmiary —
          nie ściska suwaka ani pól do zera. */}
      <div
        data-workflow-controls
        className="ml-auto min-w-0 shrink overflow-x-auto overflow-y-hidden"
      >
        <div className="flex w-max items-center gap-3 [&_.field]:w-44">
          {controls}
          {/* Chip z makiety (`.chip`, `data-copy`): czas i koszt są wartościami maszynowymi, więc
              mono i do skopiowania. Podpowiedź nazywa, CO to za czas — suma tur agentów nie jest
              zegarem ściennym biegu i nie ma prawa go udawać.

              2026-08-31 — DWIE NAZWY ZAMIAST OŚMIU KLAS. `.chip` niesie kształt pigułki (obrys,
              wypełnienie, promień, 20 px), `.value` — krój maszynowy razem ze stopniem i
              `tabular-nums`. Ręczny zapis stał na własnej wysokości 19 px i własnym paddingu,
              czyli był chipem o innych wymiarach niż każdy inny chip w aplikacji, a rodzinę mono
              deklarował drugi raz obok stopnia, który ją już niesie. Liczba, która nie skacze
              przy odświeżeniu, jest tu treścią: suma tur zmienia się co kilka sekund. */}
          {strip.spend === '' ? null : (
            <span
              data-copyable
              title="Time the agents have spent on this run, and what it has cost so far"
              className="chip value"
            >
              {strip.spend}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
