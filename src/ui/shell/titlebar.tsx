/* Boczne menu — jedyna nawigacja, jaką ta aplikacja ma.
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
 * Która sekcja jest otwarta, jest powiedziane DOKŁADNIE RAZ: przez `aria-current` na
 * przełączniku (niezmiennik 13). Wygląd aktywnego przycisku bierze się z tego samego atrybutu
 * — wariant `aria-[current=true]:` czyta DOM, zamiast trzymać drugą kopię tej samej prawdy
 * w klasie. poprzedni prototyp pokazywał stan połączenia w sześciu miejscach naraz [03 §4.4].
 */
import type { ReactElement } from 'react';
import type { Section } from '../sections';
import { SECTIONS } from '../sections';
import { Mark } from '../brand/mark';
import { NavIcon } from './nav-icons';
import { FIRST_SECTION, useSectionStore } from './section-store';
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

/** Szerokość bocznego menu. Wartość z `docs/mockup/index.html`, reguła `.app`. */
export const NAV_WIDTH = 208;

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
 * Do 2026-08-18 stało tu „Claude · Codex ready" i była to nieprawda o połowie otoczenia:
 * `src-tauri/src/lib.rs:288` daje Codeksowi sterownik `Absent::new("codex", "T-10")`, którego
 * `probe` oddaje `found: false`, a `start` odmawia. Aplikacja meldowała gotowość rzeczy, która
 * przy pierwszym kroku odmawia — czyli dokładnie ten rodzaj kłamiącej kontrolki, po którym
 * człowiek przestaje wierzyć całemu ekranowi.
 *
 * DLACZEGO NAPIS, A NIE ODCZYT. `src-tauri/commands.golden.txt` nie ma dziś ANI JEDNEJ komendy,
 * która pyta o stan dostawców — `probe` istnieje na sterowniku i nie jest wystawiony na granicę.
 * Napis, który nie ma skąd wziąć prawdy, ma mówić to, co jest pewne, a nie to, co brzmi lepiej;
 * prawdziwy odczyt jest zgłoszony orkiestratorowi jako komenda do dopisania. Kiedy powstanie,
 * ta stała zniknie razem z zaszytą wiedzą o vendorach.
 *
 * Zbiór dostawców, których tu nie ma, NIE jest wpisany z palca w kryterium: `nav-furniture.test.tsx`
 * czyta `Absent::new("…")` z `lib.rs` w tym samym biegu testu i porównuje z tym zdaniem, więc
 * dzień, w którym Codex naprawdę zacznie działać, jest dniem, w którym ten test świeci na
 * czerwono, dopóki zdanie go nie nazwie.
 */
const READY = 'Claude ready';

export interface SideNavProps {
  section?: Section;
}

export function SideNav({ section = FIRST_SECTION }: SideNavProps): ReactElement {
  return (
    <nav
      data-chrome
      data-tauri-drag-region
      className="pane flex min-h-0 shrink-0 flex-col px-2 pb-[10px]"
      style={{ width: NAV_WIDTH, paddingTop: CHROME_INSET_TOP }}
    >
      <div className="flex items-center gap-[10px] px-2 pb-4">
        <Mark />
        {/* LOGOTYP MAŁYMI LITERAMI od 2026-08-19. `LOADOUT` w monospace z rozstrzeleniem
            `.12em` było cytatem z terminala, nie logotypem: mono w tym systemie znaczy „to
            wyprodukowała maszyna", a nazwa produktu jest językiem ludzkim. Dom pisze `murmur`
            dokładnie tak — Hanken Grotesk 600, ciasny tracking. */}
        <b className="text-heading text-ink">loadout</b>
      </div>

      {/* PRZEŁĄCZNIK ZAKRESU STOI MIĘDZY ZNAKIEM A LISTĄ SEKCJI, i ta kolejność jest treścią,
          nie układem: boczne menu odpowiada na „co robię", a zakres mówi, GDZIE to robię —
          czyli jest ramą dla wszystkich pięciu sekcji, nie szóstą z nich. Postawiony pod nimi
          czytałby się jak jeszcze jedno miejsce, do którego się wchodzi.

          Poniżej nic go nie zasłania i nic o nim nie wie: `SideNav` zostaje bezstanowy poza
          propsem `section`, a cały stan zakresu mieszka w `workspace-switcher.tsx`. */}
      <NavWorkspaces />

      {SECTIONS.map((entry) => (
        <button
          key={entry.id}
          type="button"
          data-section-switch={entry.id}
          aria-current={entry.id === section ? 'true' : undefined}
          onClick={() => useSectionStore.getState().go(entry.id)}
          className="group grid w-full grid-cols-[auto_1fr] items-center gap-[9px] rounded-sm border border-transparent px-[10px] py-[7px] text-left text-ui text-body aria-[current=true]:bg-hover aria-[current=true]:text-ink"
        >
          {/* AKCENT BIERZE GLIF, NIGDY TŁO. To reguła domu, wprost z jego `glass.css`:
              „the accent never fills chrome, it colors the active glyph/label only". Barwa jest
              BRAMKOWANA wariantem `group-aria-[current=true]`, a nie policzona drugim razem
              w TSX — bo która sekcja jest otwarta, mówi `aria-current` i tylko on
              (niezmiennik 13). Ternary tutaj byłby drugą kopią tej samej decyzji. */}
          <span className="text-muted group-aria-[current=true]:text-accent">
            <NavIcon section={entry.id} />
          </span>
          <span>{entry.label}</span>
        </button>
      ))}

      {/* Stopka przypięta do dołu (`margin-top:auto` z makiety). Kropka żywotności i jedno
       * zdanie o otoczeniu — to jedyne miejsce, w którym aplikacja mówi, czym umie uruchomić
       * krok. Stopień `text-meta` to mono 11 bez rozstrzelenia, prosto z reguły `.foot`; do
       * 2026-08-18 stało tu `text-label tracking-normal`, czyli token etykiety z ręcznie
       * zniesioną połową jego własnej definicji, bo tego stopnia w drabince nie było. */}
      <div className="mt-auto flex items-center gap-[7px] border-t border-line px-[10px] pt-[10px] font-mono text-meta text-muted">
        {/* Kropka gotowości jest PRZYGASZONA od 2026-08-19. Akcent znaczy „to jest
            interaktywne", a dostępność dostawcy nie jest ani interakcją, ani „teraz"
            (DESIGN §3). Nie pulsuje też: pulsuje wyłącznie kropka pracującego agenta,
            a sufit z ARCHITECTURE §7 daje dwa regiony animujące się od jednego zdarzenia. */}
        <span aria-hidden className="size-[7px] rounded-full bg-muted" />
        <span>{READY}</span>
      </div>
    </nav>
  );
}
