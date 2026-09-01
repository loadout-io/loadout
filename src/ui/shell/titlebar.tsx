/* Boczne menu — jedyna nawigacja, jaką ta aplikacja ma, w dwóch trybach.
 *
 * DLACZEGO Z BOKU, A NIE NA GÓRZE — i to nie jest kwestia gustu. `docs/ARCHITECTURE.md` §7
 * mówi wprost: „**Boczne menu** odpowiada na »co robię« (Praca / Workflow / Agenci /
 * Umiejętności / Pamięć), karty odpowiadają na »w którym folderze«", i podaje budżet chrome:
 * „Karty 34 px + pasek loadoutu 56 px = **90 z 96**. Zostało sześć pikseli."
 *
 * Do 2026-08-17 stał tu pasek POZIOMY, `TITLEBAR_HEIGHT = 48`, wsadzony nad treść. To dawało
 * 48 + 34 + 56 = **138 px chrome przy suficie 96** — 1,44× limitu, który ten sam paragraf
 * nazywa nienegocjowalnym. Nie był to błąd wykonania: kontrakt T-01 zażądał dokładnie tego
 * („Chrome nad pierwszą treścią: JEDEN PASEK, TITLEBAR_HEIGHT = 48"), powołując się na §7,
 * ale na jego zły akapit — i własne kryterium to zabetonowało. Makieta
 * (`docs/mockup/index.html`) opisywała boczne menu 196 px od początku i nikt jej nie czytał,
 * bo nic nie było zbudowane, żeby na nią patrzeć.
 *
 * Menu z boku NIE jest chrome nad treścią: stoi OBOK, więc do sufitu z §7 wnosi zero.
 *
 * ── DWA TRYBY, NIE DWIE NAWIGACJE, 2026-08-31 ────────────────────────────────────────────────
 *
 * Do tego dnia ten plik rysował DWIE kontrolki na tę samą pracę i obie naraz: wąską kolumnę
 * glifów (`data-jump`) i listę wierszy (`data-section-switch`). Obie wołały
 * `useSectionStore.go(entry.id)`, więc każde z siedmiu miejsc miało dwie drogi stojące obok
 * siebie — jeden fakt, dwa nośniki (niezmiennik 13). Zdanie właściciela: „nawigacja na pasku
 * vs ta sidebar to to samo, więc możemy zrobić z tego 2 mode, jeden dla collapsed, drugi
 * expanded".
 *
 * Zostaje JEDNA nawigacja i JEDEN znacznik na sekcję — `data-section-switch` — który w trybie
 * rozwiniętym niesie wiersz, a w zwiniętym ikona. Tego znacznika nie wolno ani zgubić, ani
 * podwoić w żadnym trybie: nim wchodzi na sekcje każde kryterium przeglądarkowe z `e2e/`.
 *
 * CO PRZEŻYWA ZWĘŻENIE, I DLACZEGO AKURAT TO. Reguła jest jedna: rzecz znika tylko wtedy, kiedy
 * jej brak NIE KŁAMIE.
 *
 *   kłódka        ZOSTAJE, jako plakietka na ikonie. Bez niej człowiek klika w sekcję, która
 *                 nic nie pokaże, i nie dowiaduje się dlaczego. To jedyna rzecz, którą
 *                 właściciel wskazał z nazwy.
 *   powód kłódki  ZOSTAJE, w `title` i w nazwie dostępnej. „Nie wolno" bez „oto co by to
 *                 zmieniło" jest dokładnie tym zdaniem, które ta nawigacja miała skasować.
 *   plakietka     ZOSTAJE, jako kropka na ikonie Run. To jedyne miejsce, w którym aplikacja
 *   żywego biegu  z KAŻDEGO ekranu przyznaje, że coś pracuje; zwężenie kolumny nie jest
 *                 powodem, żeby to schować.
 *   etykieta      ZOSTAJE jako nazwa dostępna i podpowiedź. Siedem nieopisanych kształtów to
 *                 zgadywanka dla oka i cisza dla czytnika ekranu.
 *   klawisz ⌘n    ZOSTAJE w podpowiedzi, znika z widoku. Klawiatura bierze go tak samo w obu
 *                 trybach, więc obietnica nie ginie — ginie tylko jej rysunek, na który
 *                 w kwadracie 38 px nie ma miejsca.
 *   liczniki      ZNIKAJĄ. Liczba w kwadracie 38 px jest nieczytelna, a jej brak nie twierdzi
 *                 niczego fałszywego: „ile mam agentów" odpowiada sama sekcja. Zero i tak
 *                 nigdy nie było rysowane.
 *   nadoczka      ZNIKAJĄ jako słowa i ZOSTAJĄ jako odstęp. Grupa dalej widać — „te trzy
 *   MAKE/RUN/KNOW należą do siebie" — tylko bez nazwy, której nie ma gdzie napisać.
 *   karta         ZNIKA. To rada, nie fakt o danych; jej brak nie mówi nieprawdy, a wraca
 *   NEXT STEP     jednym klawiszem. Świeża instalacja i tak otwiera się rozwinięta.
 *   przełącznik   ZNIKA. Nazwa zakresu jest słowem, a rozwijana lista nazw nie mieści się
 *   zakresu       w kolumnie 64 px. Brak nie kłamie: nie odpowiada na pytanie, zamiast
 *                 odpowiadać na nie źle — a odpowiedź jest o jeden klawisz stąd.
 *   zdanie stopki ZNIKA, kropka ZOSTAJE razem z podpowiedzią. Ucięta lista dostawców byłaby
 *                 obietnicą gotowości kogoś, kogo na niej nie widać.
 *
 * Która sekcja jest otwarta, jest powiedziane DOKŁADNIE RAZ: przez `aria-current` na
 * przełączniku (niezmiennik 13) — w obu trybach tym samym atrybutem. Wygląd aktywnego przycisku
 * bierze się z tego samego atrybutu: wariant `aria-[current=true]:` czyta DOM, zamiast trzymać
 * drugą kopię tej samej prawdy w klasie. poprzedni prototyp pokazywał stan połączenia w sześciu miejscach
 * naraz [03 §4.4].
 */
import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';
import type { Purpose, Section, SectionEntry } from '../sections';
import { SECTIONS } from '../sections';
import { useRun } from '../../state/run';
import { collapseNav, navIsCollapsed, subscribeToNavCollapsed } from '../../state/settings';
import { useWorkspaces } from '../../state/workspaces';
import { Mark } from '../brand/mark';
import { LockGlyph, NavIcon, PanelGlyph, SearchGlyph } from './nav-icons';
import { askForSearch } from './search-asked';
import { FIRST_SECTION, useSectionStore } from './section-store';
import type { WhatYouHave } from './what-you-have';
import { useWhatYouHave } from './what-you-have';
import { NavWorkspaces } from './workspace-switcher';

/* Trzy nazwane stale, ktore do 2026-08-19 byly LICZBAMI W KOMENTARZU. Liczba w komentarzu jest
 * wartoscia, ktorej nie da sie sprawdzic, a te trzy wchodza w jedna arytmetyke z `CHROME_INSET_TOP`
 * i z `trafficLightPosition` w `tauri.conf.json`. Kryterium AC-2 liczy te sume z CZTERECH
 * odczytow, wiec zadna z nich nie jest juz wpisana dwa razy.
 */

/** Wysokosc swiatel macOS, zmierzona na oknie. */
export const LIGHTS_HEIGHT = 20;

/** Odstep pod swiatlami, zeby marka ich nie dotykala. */
export const LIGHTS_GAP = 8;

/**
 * O ile kartki plywaja od krawedzi okna i od siebie — jeden stopien skali odstepow (`--space-2`).
 *
 * DLACZEGO 8, A NIE 6. Skala tego systemu ma baze 4 px, wiec 6 px nie ma w niej stopnia i wyszlo
 * by z tego `p-[6px]`, czyli wartosc arbitralna — ta sama ucieczka, ktora `quick-tokens.sh`
 * zamyka dla barw i promieni. 8 px jest na skali i zostawia trzy piksele zapasu w budzecie
 * chrome: 8 + 1 (obrys kartki) + 32 (karty) + 52 (pasek) = 93 przy suficie 96 z ARCHITECTURE §7.
 */
export const PANE_GAP = 8;

/**
 * Szerokość bocznego menu ROZWINIĘTEGO. Wartość z `docs/mockup/index.html`, reguła `.app`.
 *
 * 208 → 308 (2026-08-31). Liczbę czyta z makiety `shell-matches-mockup.test.tsx` i porównuje
 * z tym, co powłoka naprawdę rysuje, więc nie da się przesunąć jednej bez drugiej.
 */
export const NAV_WIDTH = 308;

/**
 * Szerokość bocznego menu ZWINIĘTEGO do samych ikon. Reguła `.app[data-narrow]` z tej samej
 * makiety i czytana tym samym kryterium, co ta wyżej.
 *
 * SKĄD 64, i to jest arytmetyka, nie gust: 1 (obrys kartki) + 8 (`px-2`) + 38 (bok przycisku)
 * + 8 + 1 = 56, czyli cztery piksele luzu z każdej strony przycisku. Węższa kolumna dociskałaby
 * glif do krawędzi szkła, szersza przestałaby się czytać jako pasek ikon.
 */
export const NAV_NARROW = 64;

/**
 * Górny odstęp kartki nawigacji: światła macOS pływają NAD treścią (`titleBarStyle: "Overlay"`,
 * `hiddenTitle: true`), a ich lewy górny róg to `trafficLightPosition` z `tauri.conf.json`.
 * Marka zaczyna się dopiero pod nimi, inaczej leży pod światłami i jest nieczytelna.
 *
 * Od T-46 kartka pływa o `PANE_GAP` niżej niż okno, więc jej własny odstęp MALEJE o tyle samo:
 *
 *   16 (`trafficLightPosition.y`) + 20 (`LIGHTS_HEIGHT`) + 8 (`LIGHTS_GAP`) − 8 (`PANE_GAP`) = 36
 *
 * Te cztery liczby są **związane i mierzone razem** przez kryterium AC-2, które czyta pierwszą
 * z `tauri.conf.json`, czwartą z makiety, a dwie środkowe bierze z eksportów powyżej. Zmiana
 * jednej bez pozostałych jest czerwona; osobno każda wygląda rozsądnie i właśnie dlatego marka
 * leżała pod światłami przez trzy dni w repo źródłowym [T8 §11, 2026-08-15].
 */
export const CHROME_INSET_TOP = 36;

/**
 * Zdanie w stopce. Mówi o tym, czym ta aplikacja NAPRAWDĘ umie uruchomić krok.
 *
 * ZMIERZONE 2026-08-21: fabryka w `src-tauri/src/lib.rs` daje obu dostawcom ich prawdziwe
 * adaptery. `nav-furniture.test.tsx` czyta unię dostawców i mapowanie `Absent` w tym samym
 * biegu, więc kolejna rozbieżność między stopką a runtime'em przewróci test.
 *
 * DLACZEGO NAPIS, A NIE ODCZYT. `src-tauri/commands.golden.txt` nie ma dziś ANI JEDNEJ komendy,
 * która pyta o stan dostawców — `probe` istnieje na sterowniku i nie jest wystawiony na granicę.
 * Kiedy taki odczyt powstanie, ta stała zniknie razem z zaszytą wiedzą o vendorach.
 */
const READY = 'Claude · Codex ready';

/** Napis na kontrolce zwijania, w obu kierunkach. Wprost z domu (wiersz „Collapse sidebar"). */
const FOLD = 'Collapse sidebar';
const UNFOLD = 'Expand sidebar';

/** Klawisz, którym zwija się panel wszędzie, gdzie człowiek ten gest już zna. */
const FOLD_KEY = '⌘B';

export interface SideNavProps {
  section?: Section;
}

/** Czy w zakresie, na który patrzy człowiek, coś biegnie. Stała tożsamość, bo jest migawką. */
const runIsGoing = (): boolean => useRun.getState().workflow !== '';

/** W którym zakresie człowiek pracuje. Też migawka, więc też stała tożsamość. */
const whereWeWork = (): string | null => useWorkspaces.getState().activeId;

export function SideNav({ section = FIRST_SECTION }: SideNavProps): ReactElement {
  /* `getState` w OBU migawkach, nie hak zustanda. Powód jest zmierzony i zapisany w
   * `src/sections/workflows/index.tsx`: `renderToStaticMarkup` bierze migawkę SERWEROWĄ, a ta
   * u zustanda jest stanem z chwili utworzenia magazynu — komponent czytający hakiem byłby
   * w każdym kryterium pusty. To okno nigdy nie hydratuje serwerowego HTML-a, więc powód,
   * dla którego zustand oddaje tam stan początkowy, tutaj nie istnieje. */
  const have = useSyncExternalStore(
    useWhatYouHave.subscribe,
    useWhatYouHave.getState,
    useWhatYouHave.getState,
  );
  /* Czy COKOLWIEK biegnie — jedno pole, `workflow`, i to jest jedyna odpowiedź całej aplikacji
   * na to pytanie (`src/state/run.ts`, niezmiennik 13). Uchwyt `useRun` sam przepina się na
   * sesję zakresu, w którym człowiek właśnie pracuje.
   *
   * MIGAWKĄ JEST WARTOŚĆ LOGICZNA, NIE CAŁY STAN, i to jest różnica wydajności, nie stylu:
   * bieg dosypuje do magazynu wiersz za wierszem, a migawka oddająca cały obiekt ma za każdym
   * razem nową tożsamość — czyli menu przerysowywałoby się na KAŻDĄ linię strumienia.
   * `false → true` zdarza się raz na bieg. */
  const going = useSyncExternalStore(useRun.subscribe, runIsGoing, runIsGoing);

  /* Który tryb. Jedno pytanie do jedynego miejsca, które zna odpowiedź — a to miejsce jest tym
   * samym plikiem, który pamięta domyślnego lidera i sufit wydatku (`src/state/settings.ts`),
   * więc wybór przeżywa zamknięcie okna bez drugiego sposobu zapisu ustawień. */
  const collapsed = useSyncExternalStore(subscribeToNavCollapsed, navIsCollapsed, navIsCollapsed);

  /* PRZELICZENIE PRZY KAŻDYM PRZEJŚCIU, nie raz na życie okna. Kłódka przy Workflows ma zniknąć
   * w chwili, w której człowiek zapisał pierwszego agenta — a zapisał go na SĄSIEDNIEJ sekcji,
   * więc powrót tutaj jest dokładnie tym momentem, w którym dwie liczby są nieaktualne.
   * `renderToStaticMarkup` nie odpala efektów, więc w kryteriach magazyn zostaje taki, jaki
   * został zasiany — i o to chodzi.
   *
   * ZAKRES JEST DRUGIM POWODEM, bo workflow NALEŻĄ DO ZAKRESU (`list_workflows` bierze folder,
   * `src/sections/workflows/io.ts`). Bez tego przełączenie zakresu zostawiałoby kłódkę policzoną
   * z cudzej biblioteki — czyli zdanie o danych, których w tym zakresie nie ma. */
  const where = useSyncExternalStore(useWorkspaces.subscribe, whereWeWork, whereWeWork);
  useEffect(() => {
    void useWhatYouHave.getState().count();
  }, [section, where]);

  const shut = (entry: SectionEntry): boolean =>
    entry.needs !== null && have[entry.needs.shelf] === 0;

  return (
    <nav
      data-chrome
      data-tauri-drag-region
      data-nav-mode={collapsed ? 'collapsed' : 'expanded'}
      className={
        'pane flex min-h-0 shrink-0 flex-col px-2 pb-[10px]' + (collapsed ? ' items-center' : '')
      }
      style={{ width: collapsed ? NAV_NARROW : NAV_WIDTH, paddingTop: CHROME_INSET_TOP }}
    >
      <div
        className={'flex items-center pb-3' + (collapsed ? ' justify-center' : ' gap-[10px] px-2')}
      >
        <Mark />
        {/* LOGOTYP MAŁYMI LITERAMI od 2026-08-19. `LOADOUT` w monospace z rozstrzeleniem
            `.12em` było cytatem z terminala, nie logotypem: mono w tym systemie znaczy „to
            wyprodukowała maszyna", a nazwa produktu jest językiem ludzkim. Znika przy zwężeniu,
            znak zostaje: tożsamość jest kształtem, nie słowem. */}
        {collapsed ? null : <b className="text-heading text-ink">loadout</b>}
      </div>

      {/* PRZEŁĄCZNIK ZAKRESU STOI MIĘDZY ZNAKIEM A LISTĄ MIEJSC, i ta kolejność jest treścią,
          nie układem: boczne menu odpowiada na „co robię", a zakres mówi, GDZIE to robię —
          czyli jest ramą dla wszystkich siedmiu sekcji, nie ósmą z nich. Postawiony pod nimi
          czytałby się jak jeszcze jedno miejsce, do którego się wchodzi.

          Poniżej nic go nie zasłania i nic o nim nie wie: `SideNav` zostaje bezstanowy poza
          propsem `section`, a cały stan zakresu mieszka w `workspace-switcher.tsx`. */}
      {collapsed ? null : <NavWorkspaces />}

      {collapsed ? (
        <IconNav section={section} shut={shut} going={going} />
      ) : (
        <PlaceList section={section} shut={shut} have={have} going={going} />
      )}

      <FoldControl collapsed={collapsed} />

      {/* Stopka przypięta do dołu (`margin-top:auto` z makiety). Kropka żywotności i jedno
       * zdanie o otoczeniu — to jedyne miejsce, w którym aplikacja mówi, czym umie uruchomić
       * krok. Stopień `text-meta` to mono 11 bez rozstrzelenia, prosto z reguły `.foot`; do
       * 2026-08-18 stało tu `text-label tracking-normal`, czyli token etykiety z ręcznie
       * zniesioną połową jego własnej definicji, bo tego stopnia w drabince nie było.
       *
       * W trybie zwiniętym zostaje sama kropka, a zdanie przenosi się do podpowiedzi: lista
       * dostawców ucięta do dwóch znaków obiecywałaby gotowość kogoś, kogo nie widać. */}
      <div
        title={READY}
        className={
          'mt-auto flex items-center border-t border-line pt-[10px] font-mono text-meta text-muted' +
          (collapsed ? ' w-full justify-center' : ' gap-[7px] px-[10px]')
        }
      >
        {/* Kropka gotowości jest PRZYGASZONA od 2026-08-19. Akcent znaczy „to jest
            interaktywne", a dostępność dostawcy nie jest ani interakcją, ani „teraz"
            (DESIGN §3). Nie pulsuje też: pulsuje wyłącznie kropka pracującego agenta,
            a sufit z ARCHITECTURE §7 daje dwa regiony animujące się od jednego zdarzenia. */}
        <span aria-hidden className="size-[7px] rounded-full bg-muted" />
        {collapsed ? null : <span>{READY}</span>}
      </div>
    </nav>
  );
}

/* GRUPY SĄ WYPROWADZONE Z REJESTRU, nie spisane drugi raz. Sekcja dopisana do `SECTIONS`
 * z polem `purpose` wchodzi do swojej grupy sama; sekcja bez niego ląduje pod listą, przy
 * Settings. Druga tablica par „grupa → pozycje" rozjechałaby się z rejestrem przy pierwszej
 * dopisanej sekcji, a rozjazd czyta się jak zgubione miejsce. */
interface Group {
  readonly purpose: Exclude<Purpose, null>;
  readonly of: readonly SectionEntry[];
}

const GROUPS: readonly Group[] = (() => {
  const out: Group[] = [];
  for (const entry of SECTIONS) {
    if (entry.purpose === null) continue;
    const last = out[out.length - 1];
    if (last !== undefined && last.purpose === entry.purpose) {
      (last.of as SectionEntry[]).push(entry);
      continue;
    }
    out.push({ purpose: entry.purpose, of: [entry] });
  }
  return out;
})();

/** Pozycje spoza trzech grup, w kolejności rejestru. */
const LOOSE: readonly SectionEntry[] = SECTIONS.filter((entry) => entry.purpose === null);

/** Który to `⌘`. Pozycja w rejestrze i tylko ona — ten sam odczyt robi `../palette/keys.ts`. */
function keyFor(entry: SectionEntry): string {
  return '⌘' + String(SECTIONS.findIndex((one) => one.id === entry.id) + 1);
}

interface ModeProps {
  readonly section: Section;
  readonly shut: (entry: SectionEntry) => boolean;
  readonly going: boolean;
}

/**
 * TRYB ROZWINIĘTY: lista miejsc pogrupowana pod nadoczkami, z lupą, licznikami i radą.
 *
 * Płaska lista siedmiu równych pozycji mówi wyłącznie „tu jesteś" i nic o tym, w jakiej
 * kolejności te siedem miejsc ma dla człowieka sens. Grupy mówią PO CO się tu przychodzi —
 * zrobić, uruchomić, wiedzieć.
 */
function PlaceList({
  section,
  shut,
  have,
  going,
}: ModeProps & { readonly have: WhatYouHave }): ReactElement {
  return (
    <div className="flex min-h-0 w-full flex-1 flex-col overflow-y-auto">
      <div className="flex items-center justify-between border-b border-line px-1 pb-[10px]">
        <b className="text-heading text-ink">Browse</b>
        {/* LUPA JEST DRUGIMI DRZWIAMI DO TEGO SAMEGO. `⌘K` otwiera paletę od zawsze i jest
            jedyną drogą, którą ta aplikacja miała — a skrót, którego się nie zna, nie
            istnieje. Prośba jedzie przez `search-asked.ts`, więc menu nie wie o palecie nic
            poza tym, że ktoś ją odbierze. */}
        <button
          type="button"
          data-nav-search
          aria-label="Search, or jump to anything"
          title="Search, or jump to anything"
          onClick={askForSearch}
          className="flex size-[26px] items-center justify-center rounded-sm text-muted transition-colors hover:bg-hover hover:text-ink"
        >
          <SearchGlyph />
        </button>
      </div>

      {GROUPS.map((group) => (
        <div key={group.purpose} className="pt-3">
          {/* NADOCZKO GRUPY. Wersaliki i rozstrzelenie niesie stopień `text-eyebrow`
              z arkusza, nie ten plik: druga deklaracja wersalików byłaby drugim miejscem
              na jeden fakt. Barwa wraca do przygaszonej — nadoczko grupy jest etykietą
              szuflady, a akcent w tym systemie znaczy „to jest interaktywne". */}
          <h2 data-nav-group className="px-2 pb-[5px] text-eyebrow text-muted">
            {group.purpose}
          </h2>
          {group.of.map((entry) => (
            <NavRow
              key={entry.id}
              entry={entry}
              open={entry.id === section}
              shut={shut(entry)}
              have={have}
              going={going}
            />
          ))}
        </div>
      ))}

      {/* SETTINGS STOI OSOBNO, NA DOLE. Nie przychodzi się tu pracować, więc pozycja pod
          żadnym z trzech nadoczek — i pod nią jedyne zdanie w całym oknie, które mówi,
          co zrobić TERAZ.

          DÓŁ ROBI ROZPÓRKA, NIE `mt-auto`, i to nie jest kwestia gustu: w tej kartce jest
          JEDEN `mt-auto` i niesie go stopka gotowości. Kryterium „kropka gotowości stoi
          w miejscu" (`src/sections/run/exactly-one-thing-pulses.test.ts`) znajduje tę stopkę
          PIERWSZYM `mt-auto` w tym pliku — drugi, postawiony wyżej, przestawiłby jego
          parser na cudzy blok i punkt przestałby sądzić to, co ma w nazwie. */}
      <span className="flex-1" />
      <div className="pt-3">
        {LOOSE.map((entry) => (
          <NavRow
            key={entry.id}
            entry={entry}
            open={entry.id === section}
            shut={shut(entry)}
            have={have}
            going={going}
          />
        ))}
        <NextStep have={have} />
      </div>
    </div>
  );
}

/**
 * TRYB ZWINIĘTY: te same siedem miejsc, zwężone do ikon.
 *
 * To NIE jest druga nawigacja stojąca obok pierwszej — to jest ta sama lista, tylko bez słów.
 * Każda ikona niesie ten sam `data-section-switch` i woła to samo przejście, co wiersz
 * w trybie obok; w drzewie stoi zawsze dokładnie jedna z tych dwóch postaci.
 *
 * Odstęp między grupami zostaje po nadoczkach, których nie ma gdzie napisać: „te trzy należą
 * do siebie" da się powiedzieć samą pustką, a pustka niczego nie nazywa źle.
 */
function IconNav({ section, shut, going }: ModeProps): ReactElement {
  return (
    <div className="flex min-h-0 w-full flex-1 flex-col items-center overflow-y-auto">
      {SECTIONS.map((entry, at) => (
        <NavGlyphButton
          key={entry.id}
          entry={entry}
          open={entry.id === section}
          shut={shut(entry)}
          going={going}
          /* Nadoczko zamienione w odstęp: pierwsza pozycja nowej grupy odsuwa się od poprzedniej.
             Settings (`purpose === null`) odpycha się rozpórką niżej, tak samo jak na liście. */
          apart={at > 0 && entry.purpose !== null && entry.purpose !== SECTIONS[at - 1]?.purpose}
        />
      ))}
      <span className="flex-1" />
    </div>
  );
}

interface RowProps {
  readonly entry: SectionEntry;
  readonly open: boolean;
  readonly shut: boolean;
  readonly have: WhatYouHave;
  readonly going: boolean;
}

/**
 * Czy ta pozycja jest naprawdę zamknięta.
 *
 * BIEG BIJE KŁÓDKĘ, i to nie jest kosmetyka. Bieg da się zacząć bez zapisanego workflow —
 * `/ask` idzie tą samą drogą — więc biblioteka może być pusta w chwili, w której coś już
 * pracuje. Wiersz z kłódką i zdaniem „Needs a workflow to run" nad żywym biegiem jest
 * po prostu nieprawdą, a nieprawda w menu kosztuje zaufanie do całej reszty.
 *
 * Jedna funkcja na oba tryby: rozjazd tej decyzji między wierszem a ikoną znaczyłby kłódkę,
 * która pojawia się i znika przy samym zwężeniu kolumny.
 */
function isShut(entry: SectionEntry, shut: boolean, going: boolean): boolean {
  return shut && !(entry.id === 'run' && going);
}

/** Czy w tym miejscu właśnie coś pracuje. Ta sama odpowiedź dla wiersza i dla ikony. */
function isLive(entry: SectionEntry, going: boolean): boolean {
  return entry.id === 'run' && going;
}

/**
 * Jeden wiersz listy miejsc.
 *
 * CO STOI W TRZECIEJ KOLUMNIE, i to jest cała reguła: pozycja, której nie da się jeszcze użyć,
 * pokazuje KŁÓDKĘ i zdanie, czego jej brakuje; miejsce, w którym coś właśnie biegnie, pokazuje
 * plakietkę biegu ZAMIAST klawisza (dwie pigułki w jednym wierszu czytają się jak dwa fakty
 * o jednym miejscu); reszta pokazuje, ile ma rzeczy i którym klawiszem się tam trafia.
 *
 * ETYKIETA JEST OSTATNIM `<span>` PRZYCISKU i to nie jest przypadek: `shell-matches-mockup`
 * czyta etykietę wiersza jako ostatni niepusty `<span>`, więc plakietka i klawisz jadą
 * w `<b>` i `<kbd>`. Zmiana tych znaczników na `<span>` cicho podmienia to, co makieta
 * i powłoka o sobie mówią.
 */
function NavRow({ entry, open, shut, have, going }: RowProps): ReactElement {
  /* ILE TEGO JEST bierze się z `holds`, a nie z `needs`: to są dwa różne pytania o tę samą
   * pozycję. `needs` mówi, czego jej brakuje, żeby dało się jej użyć — `holds` mówi, którą
   * półkę ona pokazuje. Agents nie potrzebuje niczego i ma co liczyć; Run potrzebuje workflow
   * i nie liczy nic, bo liczba biegów mieszka w pigułce obok, a nie w bibliotece. */
  const many = entry.holds === null ? null : have[entry.holds];
  const live = isLive(entry, going);
  const closed = isShut(entry, shut, going);
  return (
    <button
      type="button"
      data-section-switch={entry.id}
      aria-current={open ? 'true' : undefined}
      onClick={() => {
        useSectionStore.getState().go(entry.id);
      }}
      className={
        'row group mb-px grid grid-cols-[18px_minmax(0,1fr)_auto] gap-[10px] px-[9px]' +
        /* PRZYGASZONY JEST WIERSZ, NA KTÓRYM NIE STOISZ. Miejsce, w którym człowiek właśnie
           jest, nie ma prawa być wyszarzone — to nie jest zdanie o tym, że pozycja jest
           nieprzydatna, tylko o tym, gdzie leży wzrok. Kłódka i powód zostają w obu razach,
           bo są prawdą o danych niezależnie od tego, gdzie kto stoi. */
        (closed && !open ? ' opacity-[0.42]' : '')
      }
    >
      {/* AKCENT BIERZE GLIF, NIGDY TŁO. To reguła domu, wprost z jego `glass.css`:
          „the accent never fills chrome, it colors the active glyph/label only". Barwa jest
          BRAMKOWANA wariantem `group-aria-[current=true]`, a nie policzona drugim razem
          w TSX — bo która sekcja jest otwarta, mówi `aria-current` i tylko on
          (niezmiennik 13). Ternary tutaj byłby drugą kopią tej samej decyzji. */}
      <span className="flex text-muted group-aria-[current=true]:text-accent">
        <NavIcon section={entry.id} />
      </span>
      <span className="truncate text-left">{entry.label}</span>
      {closed ? (
        <LockGlyph />
      ) : live ? (
        <b
          data-nav-live
          className="flex items-center gap-[5px] rounded-pill bg-live-soft px-[6px] py-[2px] font-mono text-meta text-live"
        >
          {/* KROPKA STOI, CHOĆ MAKIETA JĄ MRUGA — i to jest odstępstwo policzone, nie gust.
              `docs/ARCHITECTURE.md` §7 daje DWA miejsca, które się ruszają; oba są już wydane
              (`src/sections/run/graph/tile.tsx` — pracujący krok, `src/sections/run/tabs/tab.tsx`
              — karta zakresu, w którym coś biegnie). Trzecie miejsce zapaliłoby czerwień
              w `exactly-one-thing-pulses.test.ts`, a próg podnosi się w architekturze, nie
              w komponencie. Barwa i pigułka mówią „teraz" i bez ruchu. */}
          <i className="size-[6px] rounded-full bg-live" />1
        </b>
      ) : (
        <b className="flex items-center gap-[6px]">
          {/* ILE TEGO JEST — tylko tam, gdzie liczba już coś znaczy. Zero mówi już pusty ekran,
              zdaniem i z zaproszeniem; zero w pigułce jest meblem. `null` znaczy „nikt nie
              czytał" i milczy z tego samego powodu, co kłódka. */}
          {many !== null && many > 0 ? (
            <b
              data-nav-count
              className="rounded-pill bg-hover px-[6px] py-[2px] font-mono text-meta text-body"
            >
              {many}
            </b>
          ) : null}
          <kbd className="rounded-sm border border-line bg-hover px-[5px] py-[3px] font-mono text-meta leading-none text-muted">
            {keyFor(entry)}
          </kbd>
        </b>
      )}
      {/* CZEGO BRAKUJE — pod etykietą, przez dwie kolumny, i WYŁĄCZNIE wtedy, kiedy naprawdę
          brakuje. Zdanie schowane arkuszem stylów jest zdaniem, którego żadne kryterium nie
          umie dotknąć, a policzone z niczego jest twierdzeniem o danych, których nikt nie
          widział (niezmiennik 17). */}
      {closed && entry.needs !== null ? (
        <em data-needs className="col-span-2 col-start-2 not-italic text-note text-muted">
          {entry.needs.why}
        </em>
      ) : null}
    </button>
  );
}

/**
 * Jedno miejsce zwężone do ikony — TEN SAM przełącznik, co wiersz obok.
 *
 * NAZWA DOSTĘPNA I PODPOWIEDŹ SĄ TU TREŚCIĄ, NIE UPRZEJMOŚCIĄ: kiedy etykieta znika z ekranu,
 * jedyną drogą do słowa „Workflows" jest `aria-label` i `title`. Klawisz jedzie w podpowiedzi,
 * bo klawiatura bierze go w obu trybach tak samo, a w kwadracie 38 px nie ma go gdzie narysować.
 * Powód kłódki jedzie w obu, bo „nie wolno" bez „oto co by to zmieniło" jest zdaniem, które ta
 * nawigacja miała skasować.
 */
function NavGlyphButton({
  entry,
  open,
  shut,
  going,
  apart,
}: {
  readonly entry: SectionEntry;
  readonly open: boolean;
  readonly shut: boolean;
  readonly going: boolean;
  readonly apart: boolean;
}): ReactElement {
  const closed = isShut(entry, shut, going);
  const why = closed && entry.needs !== null ? ' — ' + entry.needs.why : '';
  return (
    <button
      type="button"
      data-section-switch={entry.id}
      aria-current={open ? 'true' : undefined}
      aria-label={entry.label + why}
      title={entry.label + ' · ' + keyFor(entry) + why}
      onClick={() => {
        useSectionStore.getState().go(entry.id);
      }}
      className={
        /* TŁO AKTYWNEJ IKONY JEST NEUTRALNE, a akcent siedzi na glifie — ta sama reguła, co
           w wierszu obok i ten sam wariant `aria-current`. Wypełnienie akcentem mówiłoby „to
           jest interaktywne" o kwadracie, na którym akurat stoisz, czyli o czymś innym niż
           to, co akcent w tym systemie znaczy (DESIGN §3). */
        /* PODSWIETLONY AWATAR, nie sama zmiana tla — zgloszenie wlasciciela 2026-09-01:
           „jak collapsed to tylko male podswietlane awatary z jakims box shadow". Samo
           `bg-hover` bylo tak ciche, ze na zwinietym menu nie bylo widac, gdzie sie stoi.
           Blask ma ZEROWE przesuniecie, wiec nie jest uniesieniem i nie rusza reguly
           „plywa tylko kartka menu" (`only-the-nav-floats`). */
        'group relative flex size-[38px] shrink-0 items-center justify-center rounded-md transition-[background-color,box-shadow] hover:bg-hover aria-[current=true]:bg-hover aria-[current=true]:shadow-glyph' +
        (apart ? ' mt-3' : ' mt-[2px]') +
        (closed && !open ? ' opacity-[0.32]' : '')
      }
    >
      <span className="flex text-muted group-aria-[current=true]:text-accent">
        <NavIcon section={entry.id} big />
      </span>
      {closed ? (
        /* KŁÓDKA MUSI BYĆ WIDOCZNA TAKŻE NA IKONIE — inaczej człowiek klika w sekcję, która
           nic nie pokaże, i nie wie dlaczego. Sama przygaszona ikona tego nie mówi: przygaszenie
           czyta się jak „nie tutaj jesteś", nie jak „tego nie da się jeszcze użyć". */
        <span data-nav-locked className="absolute -right-[2px] -bottom-[2px] flex text-muted">
          <LockGlyph />
        </span>
      ) : isLive(entry, going) ? (
        /* Plakietka „1 live" nie mieści się w kwadracie, więc zostaje z niej sama barwa i sama
           kropka. Kropka STOI, nie mruga — dwa regiony ruchu z ARCHITECTURE §7 są już wydane. */
        <i
          data-nav-live
          className="absolute top-[3px] right-[3px] size-[6px] rounded-full bg-live"
        />
      ) : null}
    </button>
  );
}

/**
 * Kontrolka, która zwija i rozwija menu — jedna, w obu kierunkach.
 *
 * NA DOLE PANELU, tak jak w domu (`../meetnotes`, wiersz „Collapse sidebar" pod listą). Stoi
 * POZA przewijaną listą miejsc, więc jest widoczna niezależnie od tego, dokąd człowiek
 * przewinął — kontrolka wyjścia z trybu, którą trzeba najpierw znaleźć, jest pułapką.
 *
 * KLAWISZ JEST NARYSOWANY, nie opisany w dokumentacji: skrót, którego się nie zna, nie
 * istnieje. Klawiaturę obsługuje `../palette/keys.ts` i to ona czyni tę obietnicę prawdziwą.
 */
function FoldControl({ collapsed }: { readonly collapsed: boolean }): ReactElement {
  const say = collapsed ? UNFOLD : FOLD;
  return (
    <div className={'pt-[10px]' + (collapsed ? '' : ' w-full')}>
      <button
        type="button"
        data-nav-fold
        aria-label={say}
        title={say + ' · ' + FOLD_KEY}
        onClick={() => {
          /* Odpowiedź jest ZGUBIONA ŚWIADOMIE — powód w całości stoi przy `collapseNav`
             (`src/state/settings.ts`): menu przestawia się natychmiast, a zdanie, które ta
             funkcja oddaje, mówi wyłącznie o tym, czy plik zdążył to zapamiętać. Nieudany
             zapis kosztuje jedno: przy następnym uruchomieniu menu wraca w poprzednim trybie. */
          void collapseNav(!collapsed);
        }}
        className={
          collapsed
            ? 'flex size-[38px] items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-ink'
            : 'row grid w-full grid-cols-[18px_minmax(0,1fr)_auto] gap-[10px] px-[9px] text-muted'
        }
      >
        <span className="flex">
          <PanelGlyph />
        </span>
        {collapsed ? null : (
          <>
            <span className="truncate text-left">{say}</span>
            <kbd className="rounded-sm border border-line bg-hover px-[5px] py-[3px] font-mono text-meta leading-none text-muted">
              {FOLD_KEY}
            </kbd>
          </>
        )}
      </button>
    </div>
  );
}

/**
 * JEDNO ZDANIE O TYM, CO ZROBIĆ TERAZ — i znika, kiedy nie ma już czego doradzać.
 *
 * To jest druga połowa odpowiedzi na „nie wiem, od czego zacząć". Kłódka mówi, czego brakuje
 * TEJ pozycji; ten panel mówi, co zrobić W OGÓLE — i mówi to z tych samych dwóch liczb, więc
 * nie ma jak obiecać kroku, który jest już zrobiony. Rada, która nigdy się nie kończy,
 * przestaje być radą i staje się meblem, którego nikt nie czyta.
 */
function NextStep({ have }: { readonly have: WhatYouHave }): ReactElement | null {
  const say =
    have.agents === 0
      ? 'Make one agent. Everything else in this list opens up the moment you do.'
      : have.agents !== null && have.workflows === 0
        ? 'Open Workflows and put two agents in a row. Two steps is already a workflow.'
        : null;
  if (say === null) return null;
  return (
    <div className="mt-[10px] rounded-md border border-accent-edge bg-accent-soft p-[10px] text-note text-body">
      <b className="mb-1 block text-eyebrow text-accent">Next step</b>
      <span data-next-step>{say}</span>
    </div>
  );
}
