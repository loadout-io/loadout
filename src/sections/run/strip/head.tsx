/* Nagłówek ekranu biegu — `.rhead` z `docs/mockup/index.html`, narysowany.
 *
 * Komponent jest głupi z premedytacją: dostaje gotowe `Headline` z `./headline` i je rysuje.
 * Ani jednego `if` o stanie kroku, ani jednej liczby policzonej tutaj — co ekran mówi o biegu,
 * rozstrzyga model, bo to repo nie ma jsdom i tylko tam da się to sprawdzić bez okna.
 *
 * ── GDZIE TEN NAGŁÓWEK STOI, I DLACZEGO TO NIE JEST OBEJŚCIE SUFITU ──────────────────────────
 *
 * Stoi WEWNĄTRZ `[data-work]`, jako rząd biorący obie kolumny — czyli dokładnie tam, gdzie
 * rysuje go makieta (pas na całą szerokość nad ścieżką kroków i strumieniem), ale po TREŚCIOWEJ
 * stronie kotwicy, którą `scripts/density-collect.mjs` uważa za pierwszą treść ekranu.
 *
 * Powód jest zmierzony i stoi w raporcie do właściciela: makieta stawia `.rhead` NAD `.work`,
 * a jej własny ekran biegu wydaje wtedy **222 px** nad pierwszą treścią przy suficie **96**
 * z `docs/ARCHITECTURE.md` §7 (sam `.rhead` mierzy 101 px, a aplikacja ma dziś 3 px zapasu:
 * 8 + 1 + 32 + 52 = 93). Postawienie tego pasa nad `[data-work]` nie mieści się w budżecie
 * przy ŻADNEJ typografii — nie jest to więc kwestia stopnia tytułu.
 *
 * Rozstrzygnięcie, które tu zapadło: tożsamość biegu (jak się nazywa, czy idzie, od kiedy,
 * w jakim workspace, ile wydał) JEST treścią tego ekranu, a nie jego ramą. Chrome, tak jak
 * liczy go i §7, i DESIGN §5, to cztery rzeczy: odstęp okna, obrys kartki, karty workspace
 * i pasek loadoutu — wszystkie identyczne na każdym ekranie i wszystkie o APLIKACJI.
 * Nagłówek mówi o TYM biegu i znika razem z nim.
 *
 * GRANICA TEGO ROZSTRZYGNIĘCIA, powiedziana wprost, bo sprawdzenie z nieopisaną granicą jest
 * gorsze niż jego brak: gdyby właściciel uznał ten pas za chrome, liczba do porównania z §7
 * rośnie o jego wysokość i nic w kodzie tego nie zgłosi — kolektor mierzy pas nad
 * `[data-work]`, a nie „nad tym, co człowiek uzna za treść". Zgłoszone jako rozbieżność
 * makieta ↔ §7, nie obchodzone: ani jedna linia w `checks/` nie została tknięta.
 *
 * ── 2026-09-01: JEDEN PAS ZAMIAST DWÓCH, I DLACZEGO TO SIĘ TU ZMIEŚCIŁO ──────────────────────
 *
 * Zgłoszenie właściciela: „tu się za dużo dzieje […] trzeba odchudzić ten widok". Zmierzone
 * w chromium na zbudowanym `dist/`, 1512×950, scena z jednym workspace i jednym workflow
 * o dziewięciu krokach — cztery pasma nad pierwszym krokiem, razem **241 px**:
 *
 *     pasek loadoutu        52 px    chrome (liczy go `scripts/density-collect.mjs`)
 *     TEN nagłówek         104 px    treść
 *     wiersz wyboru         51 px    treść — osobny pas z własną kreską
 *     nagłówki kolumn       34 px    treść
 *
 * Środkowe dwa mówiły o JEDNEJ rzeczy: który bieg to jest i który ruszy. Wybór wszedł więc
 * w ten nagłówek, a jego pas i jego kreska zeszły. Razem z zejściem tytułu z 34 px na 22 px pas
 * nad pracą kosztuje dziś tyle, ile stoi w raporcie do właściciela; sam pomiar mieszka
 * w chromium, bo to repo nie ma jsdom, a `../index.tsx` i ten plik trzymają wyłącznie to, co da
 * się osądzić bez okna.
 *
 * ── 2026-09-01, DRUGA ZMIANA TEGO DNIA: TYTUŁEM JEST WYBÓR ───────────────────────────────────
 *
 * Nazwa workflow stała na tym ekranie w TRZECH miejscach naraz: w tytule tego nagłówka, na
 * kontrolce startu („Run Murmur-1", `../start.tsx`) i jako zaznaczona pozycja listy wyboru,
 * o wiersz wyżej od tytułu, który tę samą nazwę powtarzał. Jeden fakt, trzy nośniki
 * (niezmiennik 13). Który nośnik został i dlaczego właśnie ten, stoi w całości przy
 * [`WhichWorkflow`] w `../index.tsx`; tutaj widać z tego jedno: `RunHeadProps.chooser` wchodzi
 * W TYTUŁ, a nie obok niego, więc tytuł i wybór są jednym napisem, a nie dwoma zgodnymi.
 *
 * ── CZEGO TU NIE MA ──────────────────────────────────────────────────────────────────────────
 *
 * `Pause`. Makieta rysuje go obok `Stop`, a po stronie Rusta nie ma czego nim zawołać —
 * `src-tauri/commands.golden.txt` zna `stop_run` i `continue_run`, nie zna żadnego `pause_run`.
 * Przycisk, który nie pauzuje, jest gorszy niż jego brak (niezmiennik 16).
 *
 * `Stop`. Istnieje i jest wpięty, tylko mieszka w kontrolce startu (`../start.tsx`), razem
 * z całą polityką startu i zatrzymania. Drugi `Stop` tutaj byłby drugim miejscem, z którego
 * da się zabić bieg (niezmiennik 13).
 */
import type { ReactElement, ReactNode } from 'react';
import type { Headline } from './headline';

export interface RunHeadProps {
  headline: Headline;
  /**
   * KTÓRY WORKFLOW RUSZY — kontrolka wyboru, wstawiona W MIEJSCE tytułu.
   *
   * SLOTEM, A NIE WŁASNĄ IMPLEMENTACJĄ: co leży w katalogu i kto to wybrał, wie ekran
   * (`../index.tsx`, `WhichWorkflow`), a ten komponent jest głupi z premedytacją i rysuje to,
   * co dostał. Druga lista workflow zbudowana tutaj byłaby drugą odpowiedzią na pytanie „jakie
   * workflow istnieją" (niezmiennik 13).
   *
   * 2026-09-01 — W TYTULE, A NIE OBOK NIEGO, i to jest cała naprawa nazwy stojącej trzy razy.
   * Kontrolka stała najpierw w osobnym pasie pod nagłówkiem (51 px i własna kreska, zmierzone
   * w chromium 1512×950), potem w wierszu nadoczka — i w obu miejscach POWTARZAŁA napis, który
   * stał piętro niżej jako tytuł. Nazwa workflow ma tu jeden nośnik, więc tytuł nagłówka
   * i wybór workflow są jedną rzeczą: to, co ekran ogłasza, jest tym samym elementem, którym
   * się to zmienia.
   *
   * `null`, kiedy nie ma czego wybierać albo bieg już idzie — wtedy tytuł jest zwykłym napisem
   * (`Headline.title`), dokładnie jak w makiecie.
   */
  chooser?: ReactNode;
  /**
   * KTO TEN WORKFLOW WYBRAŁ — jedno zdanie w wierszu nadoczka.
   *
   * OSOBNYM SLOTEM, bo to jest osobne pytanie i osobny wiersz: tytuł mówi, CO ruszy, a to
   * zdanie mówi, czyja to była decyzja („Loadout picked this one for you — change it here").
   * Stoi w wierszu nadoczka, czyli tam, gdzie ekran już ogłasza stan biegu — pytanie i jego
   * pochodzenie w jednej linii, nazwa piętro niżej.
   *
   * Treść liczy `../choices.ts` (`whoChoseIt`), bo to polityka, a nie wygląd. `null`, kiedy nie
   * ma wyboru, o którym dałoby się to powiedzieć.
   */
  said?: ReactNode;
}

/**
 * Kropka stanu — ta sama średnica, co kropka `Live` w głowie strumienia
 * (`../feed/stream-head.tsx`).
 *
 * BEZ PULSU, i to jest liczba, nie gust. `docs/ARCHITECTURE.md` §7 daje **2** regiony
 * animujące się od jednego zdarzenia, a widok domyślny wydaje dziś dokładnie 2 (zmierzone
 * kolektorem `scripts/density-collect.mjs`, obie szerokości). Trzeci puls przekroczyłby sufit —
 * i mówiłby to samo, co kropka obok w głowie strumienia, czyli byłby drugim żywym regionem na
 * jeden fakt przy limicie 1. Stan niesie tu BARWA.
 */
const DOT = 'h-[6px] w-[6px] rounded-full';

/** Szerokość pudełka wydatku, `.spend` z makiety. */
const SPEND_WIDTH = 186;

/** Barwa nadoczka dla trzech stanów biegu — nośnik, nie drugi napis. */
const INK: Readonly<Record<Headline['tone'], string>> = {
  live: 'var(--color-live)',
  ended: 'var(--color-ok)',
  idle: 'var(--color-muted)',
};

export function RunHead({
  headline,
  chooser = null,
  said = null,
}: RunHeadProps): ReactElement | null {
  /* NAGŁÓWEK NAD NICZYM SIĘ NIE RYSUJE. Bez nazwy nie ma czego nazwać — a pas z pustym tytułem
     i pustą metadaną zabierałby wysokość, żeby powiedzieć, że nic nie wie (DESIGN §6). */
  if (headline.title === '') return null;

  return (
    <header
      data-run-head
      data-run-tone={headline.tone}
      className="flex min-w-0 items-start gap-5 border-b border-line px-[18px] pt-[14px] pb-[14px]"
    >
      <div className="min-w-0 flex-1">
        {/* WIERSZ NADOCZKA: stan biegu, a za nim zdanie o tym, kto wybrał to, co ruszy. Powód,
            dla którego to zdanie stoi TUTAJ, a sama nazwa piętro niżej, stoi w całości przy
            `RunHeadProps.said`. `items-center`, bo oba napisy są różnej wysokości i wyrównanie
            do góry zawiesiłoby nadoczko nad krawędzią drugiego. */}
        <div className="flex min-w-0 items-center gap-3">
          {/* NADOCZKO. Stopień niesie wersaliki i rozstrzelenie sam (`src/styles/theme.css`,
              `.text-eyebrow`), więc barwa jest jedyną rzeczą, którą dokłada ten komponent — i jest
              nośnikiem stanu, nie ozdobą. Dlaczego kropka nie pulsuje, stoi przy `DOT` wyżej.
              `shrink-0`: stan biegu jest treścią, a nie metadaną, i nie ma prawa ustąpić
              kontrolce obok (DESIGN §1). */}
          <p
            data-run-state
            className="flex shrink-0 items-center gap-2 text-eyebrow"
            style={{ color: INK[headline.tone] }}
          >
            <i aria-hidden="true" className={DOT} style={{ background: 'currentColor' }} />
            {headline.eyebrow}
          </p>

          {said}
        </div>

        {/* TYTUŁ EKRANU jako `h1` — i numer znacznika jest tu tą samą wypowiedzią, co stopień
            pisma.

            2026-08-31 — DO TEGO DNIA STAŁO TU `h2` I BYŁA TO ZMIERZONA WADA. `h1` tego ekranu
            nosiła nazwa SEKCJI („Run", 15 px, `./strip.tsx`), a nazwa biegu stała pod nią jako
            `h2` w 34 px. Oko czyta 34 px jako rzecz ważniejszą, czytnik ekranu czyta `h1` jako
            rzecz ważniejszą — a były to dwa różne napisy, więc ten sam ekran mówił dwie różne
            rzeczy zależnie od tego, czym się go czyta. Nazwa biegu była w spisie nagłówków
            PODRZĘDNA wobec etykiety paska, która mierzy pół jej wysokości.

            2026-09-01 — STOPIEŃ ZSZEDŁ Z 34 px NA 22 px (`text-hero` -> `text-title`), NA
            ZGŁOSZENIE WŁAŚCICIELA: „na pewno za duży jest ten napis workflow". Rozstrzygnięcie
            nie jest kwestią gustu i daje się powiedzieć jednym zdaniem: EKRAN MA JEDNEGO
            BOHATERA, a na ekranie biegu bohaterem jest PRACA — kroki i strumień — nie jej
            nagłówek. 34 px to stopień, którym makieta pisze tytuł ekranu drugiego rzędu
            (`h1.sm`); 22 px to jej `h2`, czyli tytuł KARTY, PANELU i OKNA DIALOGOWEGO. Nazwa
            biegu jest właśnie tym: nazwą jednej rzeczy stojącej nad pracą, a nie nazwą ekranu —
            ekran nazywa się „Run" i mówi to pasek. Rozpoznawalna, nie dominująca.

            HIERARCHIA SIĘ PRZEZ TO NIE PRZEWRACA i to jest zmierzone, nie założone. Nazwa biegu
            zostaje NAJGŁOŚNIEJSZYM nagłówkiem tego ekranu (22 px wobec 13 px nazwy sekcji
            i 11 px nadoczka „Steps") i zostaje `h1`, więc oko i spis nagłówków dalej mówią to
            samo — sądzi to `./the-eye-and-the-outline-agree.test.tsx`. Stopień 40 px
            (`text-display`) nosi w tej aplikacji dokładnie jedna rzecz: zaproszenie pierwszego
            uruchomienia (`../first-run.tsx`), gdzie tytuł JEST bohaterem, bo poza nim nie ma
            tam nic.

            MAKIETA ZMIENIŁA SIĘ RAZEM Z TYM, bo jest wyrocznią, a nie ilustracją: reguła
            `.rhead h1` w `docs/mockup/index.html` niesie od dziś ten sam stopień, co jej `h2`,
            i obie liczby czyta w jednym biegu `./the-head-of-the-run-is-one-band.test.tsx`.

            CO ZOSTAŁO BEZ CZYTELNIKA, zgłoszone, nie obchodzone: `--text-hero` w
            `src/styles/theme.css` nie ma od dziś ani jednego wołającego w `src/**` — ten tytuł
            był jedynym. Zdjęcie stopnia z drabinki należy do zadania, które ma `theme.css`
            w swoim zakresie; to nie ma go i nie tknęło ani jednej linii tamtego pliku.

            `truncate` z `title`: nazwa workflow bywa zdaniem, a metadana nie ma prawa wypychać
            treści (DESIGN §1). Pełna nazwa zostaje do przeczytania pod kursorem.

            2026-09-01 — TYTUŁEM JEST WYBÓR, KIEDY JEST CO WYBIERAĆ. Nazwa stała na tym ekranie
            trzy razy (tytuł, przycisk startu, zaznaczona pozycja listy), a stoi raz: kontrolka
            wyboru JEST tym tytułem, więc ogłoszenie i odpowiedź na nie nie mają jak się
            rozjechać (niezmiennik 13). Rozstrzygnięcie i jego granice stoją przy
            [`WhichWorkflow`] w `../index.tsx`; ten plik dostaje kontrolkę gotową i tylko ją
            wstawia. Kiedy nie ma czego wybierać — bieg IDZIE i nazwy nie da się już zmienić —
            zostaje sam napis. */}
        <h1 data-run-title className="mt-2 truncate text-title text-ink" title={headline.title}>
          {chooser ?? headline.title}
        </h1>

        {/* JEDEN WIERSZ METADANEJ, maszynowy — `.rhead .meta` z makiety. Puste człony do niego
            nie wchodzą, więc wiersz nigdy nie mówi „workspace —". */}
        {headline.meta === '' ? null : <p className="value truncate">{headline.meta}</p>}
      </div>

      {/* GRUPA PO PRAWEJ. Dziś jedna rzecz — wydatek; `Pause` i `Stop` mają swoje powody
          w nagłówku tego pliku. `shrink-0`, bo to jest liczba, a nie napis: ustępuje metadana
          obok, nigdy pomiar. */}
      {headline.spend === '' ? null : (
        <div
          data-spend
          /* `rounded-md` (13 px), a nie 12 px z makiety: promienie w tej aplikacji są tokenem,
             a nie liczbą przy komponencie (`checks/tokens.sh`), i najbliższy stopień drabinki
             jest o piksel większy. Jeden piksel promienia nie jest faktem o biegu; trzecia
             skala promieni w repo byłaby faktem o niedbałości. */
          className="ml-auto shrink-0 rounded-md border border-line bg-well px-3 py-[9px]"
          style={{ width: SPEND_WIDTH }}
        >
          <div className="flex items-baseline justify-between gap-2">
            <span className="text-eyebrow text-muted">Spend</span>
            {/* `data-copyable` zostaje na TEJ liczbie i tylko na niej: czas i koszt są
                wartościami maszynowymi, więc mono i do skopiowania. Podpowiedź nazywa, CO to za
                czas — suma tur agentów nie jest zegarem ściennym biegu i nie ma prawa go udawać. */}
            <span
              data-copyable
              data-tone="ink"
              title="Time the agents have spent on this run, and what it has cost so far"
              className="value min-w-0 truncate"
            >
              {headline.spend}
            </span>
          </div>

          {/* PASEK POSTĘPU ISTNIEJE DOKŁADNIE WTEDY, KIEDY JEST CO ZMIERZYĆ. Bez sufitu albo bez
              ani jednej wycenionej tury `used` jest `null` — a pasek narysowany wtedy pokazywałby
              ułamek policzony z niczego (niezmiennik 17). */}
          {headline.used === null ? null : (
            <div className="mt-2 h-[5px] overflow-hidden rounded-pill bg-hover">
              <i
                aria-hidden="true"
                className="block h-full rounded-pill bg-accent"
                style={{ width: String(Math.round(headline.used * 100)) + '%' }}
              />
            </div>
          )}
        </div>
      )}
    </header>
  );
}
