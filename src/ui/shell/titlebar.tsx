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
import { FIRST_SECTION, useSectionStore } from './section-store';
import { NavWorkspaces } from './workspace-switcher';

/** Szerokość bocznego menu. Wartość z `docs/mockup/index.html`, reguła `.app`. */
export const NAV_WIDTH = 196;

/**
 * Górny odstęp menu: światła macOS pływają NAD treścią (`titleBarStyle: "Overlay"`,
 * `hiddenTitle: true`), a ich lewy górny róg to `trafficLightPosition` z `tauri.conf.json`.
 * Marka zaczyna się dopiero pod nimi, inaczej leży pod światłami i jest nieczytelna.
 *
 * 16 (`trafficLightPosition.y`) + 20 (wysokość świateł) + 8 (odstęp) = 44. Makieta jest stroną
 * WWW i okna Tauri nie modeluje — ta jedna liczba jest adaptacją, nie odstępstwem. Zmiana
 * `trafficLightPosition` bez zmiany tej wartości jest czerwona w kryterium okna: te dwie liczby
 * są związane i mierzone razem, bo osobno każda wygląda rozsądnie [T8 §11, 2026-08-15].
 */
export const CHROME_INSET_TOP = 44;

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

/* Znak marki: cztery kwadraty obrócone o 45°, dwa z nich w akcencie. Prosto z makiety
 * (`.mark`), bo to jedyny element tożsamości, jaki ta aplikacja ma. */
function Mark(): ReactElement {
  return (
    <span aria-hidden className="grid size-[22px] rotate-45 grid-cols-2 grid-rows-2 gap-[2px]">
      <i className="bg-accent" />
      <i className="bg-line-strong" />
      <i className="bg-line-strong" />
      <i className="bg-accent" />
    </span>
  );
}

export function SideNav({ section = FIRST_SECTION }: SideNavProps): ReactElement {
  return (
    <nav
      data-chrome
      data-tauri-drag-region
      className="flex shrink-0 flex-col border-r border-line bg-panel px-2 pb-[10px]"
      style={{ width: NAV_WIDTH, paddingTop: CHROME_INSET_TOP }}
    >
      <div className="flex items-center gap-[10px] px-2 pb-4">
        <Mark />
        <b className="font-mono text-mono-strong text-ink">LOADOUT</b>
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
          className="w-full rounded-sq border border-transparent px-[10px] py-[7px] text-left text-ui text-body aria-[current=true]:border-line aria-[current=true]:bg-raised aria-[current=true]:text-ink"
        >
          {entry.label}
        </button>
      ))}

      {/* Stopka przypięta do dołu (`margin-top:auto` z makiety). Kropka żywotności i jedno
       * zdanie o otoczeniu — to jedyne miejsce, w którym aplikacja mówi, czym umie uruchomić
       * krok. Stopień `text-meta` to mono 11 bez rozstrzelenia, prosto z reguły `.foot`; do
       * 2026-08-18 stało tu `text-label tracking-normal`, czyli token etykiety z ręcznie
       * zniesioną połową jego własnej definicji, bo tego stopnia w drabince nie było. */}
      <div className="mt-auto flex items-center gap-[7px] border-t border-line px-[10px] pt-[10px] font-mono text-meta text-muted">
        <span aria-hidden className="size-[7px] rounded-full bg-accent" />
        <span>{READY}</span>
      </div>
    </nav>
  );
}
