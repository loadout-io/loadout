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
 * GDZIE JEST `Stop`, skoro makieta stawia go tutaj. W kontrolce biegu, na dole kolumny
 * strumienia, obok wiersza wejścia — bo tam mieszka cała polityka startu i zatrzymania:
 * wybór workflow, limit „ile naraz", zapadka na drugie kliknięcie i jedyne wywołania `io.ts`
 * (`../start.tsx`). Rozdzielenie ich dałoby dwa miejsca, z których da się ruszyć i zabić bieg
 * (niezmiennik 13), a pasek 56 px nie unosi suwaka z etykietą i ostrzeżeniem o pamięci. Makieta
 * zaczyna bieg wierszem wejścia (`/plan · /run`), czyli TĄ SAMĄ krawędzią ekranu — więc jest to
 * jej własna logika, nie ucieczka od niej. Rozbieżność zgłoszona.
 */
import type { ReactElement, ReactNode } from 'react';
import type { Block, Strip as StripModel } from './model';

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
 * Klasy bloku dla trzech stanów [DESIGN §2]: wypełniony, akcent, obrys.
 *
 * `now` jest jedynym nasyconym elementem na ekranie — reguła jednego akcentu (DESIGN §3).
 * Rzecz skończona jest cicha, więc `done` jest przygaszone, a nie zielone: zielony znaczy
 * „dzieje się teraz", nie „udało się".
 */
const BLOCK: Readonly<Record<Block['state'], string>> = {
  done: 'bg-muted',
  /* CORAL, nie akcent, od 2026-08-19. T-45 rozszczepil token: `--accent` znaczy „to jest
   * interaktywne", `--live` znaczy „to sie dzieje teraz". Segment paska jest odczytem, nie
   * kontrolka, wiec nalezy mu sie drugi z nich. */
  now: 'bg-live',
  todo: 'border border-line-strong',
};

/* Krok, który się skończył bez sukcesu, zostaje obrysem — ale obrysem PRZERYWANYM. Nie ma dla
 * niego czwartego stanu (DESIGN §2 zna trzy) i nie wolno mu dać koloru błędu: pominięty krok
 * nie jest zepsuty, a `--fail` odpowiada na pytanie „co poszło źle". Przerwana kreska mówi
 * dokładnie tyle, ile wiemy: ten krok już się nie wydarzy. */
const ENDED = 'border-dashed';

/** Etykieta bloku: akcent tylko pod krokiem, który biegnie (makieta `.blk[data-s="now"] em`). */
const LABEL: Readonly<Record<Block['state'], string>> = {
  done: 'text-muted',
  now: 'text-live',
  todo: 'text-muted',
};

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
      className="glass flex w-full shrink-0 items-center gap-[18px] border-b border-line px-[18px]"
      style={{ height: STRIP_HEIGHT }}
    >
      {/* `items-end`: bloki stoją na jednej linii z dołu, a etykiety pod nimi. Wyrównanie do
          góry rozjeżdża je, gdy jedna etykieta jest dłuższa i łamie się. */}
      {/* JEDEN SZKLANY TOREK, nie cztery luzne znaczki. Kapsula jest ksztaltem, ktory ten
          jezyk powtarza, a pasek jest jedynym miejscem, gdzie go pokazuje w chrome. `data-blocks`
          niesie tylko ten kontener — po nim kryterium pyta o material i promien. */}
      <div data-blocks className="glass flex shrink-0 items-end gap-2 rounded-pill px-2 py-[5px]">
        {strip.blocks.map((block) => (
          <span key={block.id} className="grid min-w-[38px] gap-[5px] justify-items-stretch">
            <span
              data-block={block.state}
              className={`h-2 w-full rounded-pill ${BLOCK[block.state]} ${block.ended ? ENDED : ''}`}
            />
            {/* Mono 11 bez wersalików — etykieta kroku jest jego nazwą, a nie nazwą pola,
                więc nie `text-label` (ten stopień jest w tym repo wersalikami). */}
            <span
              className={`text-center font-mono text-meta whitespace-nowrap ${LABEL[block.state]}`}
            >
              {block.name}
            </span>
          </span>
        ))}
      </div>

      <div className="min-w-0">
        {/* Nazwa sekcji na stopniu `.strip .title` z makiety (15 px / 600). Jeden rząd mniej. */}
        <h1 className="truncate text-heading text-ink">{heading}</h1>
        {/* Podpis, i tylko tutaj. Numer kroku żyje WYŁĄCZNIE na tym pasku (niezmiennik 13):
            nagłówek z własnym licznikiem i linia `done` z własnym czasem to trzy żywe regiony
            na jeden fakt przy limicie 1. */}
        {strip.caption === '' ? null : (
          <p className="truncate font-mono text-mono text-muted">{strip.caption}</p>
        )}
      </div>

      <div className="ml-auto flex min-w-0 items-center gap-3">
        {controls}
        {/* Chip z makiety (`.chip`, `data-copy`): czas i koszt są wartościami maszynowymi, więc
            mono i do skopiowania. Podpowiedź nazywa, CO to za czas — suma tur agentów nie jest
            zegarem ściennym biegu i nie ma prawa go udawać. */}
        {strip.spend === '' ? null : (
          <span
            data-copyable
            title="Time the agents have spent on this run, and what it has cost so far"
            className="inline-flex h-[19px] items-center rounded-pill border border-line bg-raised px-[7px] font-mono text-meta text-muted"
          >
            {strip.spend}
          </span>
        )}
      </div>
    </div>
  );
}
