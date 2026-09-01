/* Okno palety — czysta funkcja propsów na markup, bez własnego stanu i bez ani jednego `invoke`.
 *
 * DLACZEGO STEROWANE Z ZEWNĄTRZ. To repo nie ma jsdom, a `renderToStaticMarkup` nie odpala
 * efektów: komponent, który trzymałby wpisane słowo i podświetloną pozycję u siebie, dałby
 * kryteriom wyłącznie widok pusty i domyślny. Stan trzyma `./index.tsx`, tutaj przyjeżdża
 * propsem, więc każdą scenę tej palety da się obejrzeć bez okna.
 *
 * TO JEST MODAL, WIĘC NIE WCHODZI DO WIDOKU DOMYŚLNEGO. Sufit gęstości z ARCHITECTURE §7 mierzy
 * ekran, który zastaje pierwsze uruchomienie; ta powierzchnia istnieje w dokumencie WYŁĄCZNIE
 * po naciśnięciu ⌘K albo `?`, a `./index.tsx` przy zamkniętej palecie renderuje `null` — nie
 * schowany `<div>`, nie `hidden`, nie `display:none`. Różnica jest mierzalna: kolektor
 * (`scripts/density-collect.mjs`) liczy `[role="dialog"]` jako region i każdy element niosący
 * tekst, więc paleta schowana arkuszem stylów podniosłaby zapadkę o kilkanaście elementów przy
 * ekranie, na którym nikt jej nie widzi.
 *
 * PUŁAPKA OGNISKA JEST TU, A NIE W `./index.tsx`, bo dotyczy tego, co jest w TYM poddrzewie:
 * `Tab` na ostatniej kontrolce wraca na pierwszą, `Shift+Tab` na pierwszej idzie na ostatnią.
 * Bez tego pierwsze `Tab` wychodzi na nawigację pod przyciemnieniem — czyli na kontrolki,
 * których człowiek w tym momencie nie widzi i nie może kliknąć.
 */
import { useRef } from 'react';
import type { KeyboardEvent as Typed, ReactElement } from 'react';

import { insideMove } from './keys';
import type { PaletteItem } from './items';
import { keyOf } from './items';
import type { Shortcut } from './shortcuts';

/** Która z dwóch list stoi w oknie. */
export type Showing = 'items' | 'shortcuts';

export interface PaletteProps {
  readonly showing: Showing;
  /** Co człowiek wpisał. Zawęża OBIE listy, więc lista skrótów też jest przeszukiwalna. */
  readonly typed: string;
  /** Pozycje po zawężeniu, w kolejności ważności (`./items.ts`). */
  readonly items: readonly PaletteItem[];
  /** Skróty po zawężeniu (`./shortcuts.ts`). */
  readonly rows: readonly Shortcut[];
  /** Która pozycja jest podświetlona. */
  readonly at: number;
  /** Czy biblioteka odmówiła odczytu — wtedy w oknie stoją same sekcje i mówimy o tym wprost. */
  readonly unread: boolean;
  readonly onType: (typed: string) => void;
  readonly onStep: (by: number) => void;
  readonly onChoose: (item: PaletteItem) => void;
  readonly onShow: (showing: Showing) => void;
  readonly onClose: () => void;
}

/* Przyciemnienie niesie `.fade-in`, nie `.enter`: DESIGN §6 mówi o modalu wprost — bez rozmycia
 * i bez wjazdu poza przezroczystość. Sprężyna należy do powierzchni, które WCHODZĄ w widok,
 * a to okno zasłania go w całości. Jeden region na to zdarzenie (ARCHITECTURE §7 dopuszcza dwa). */
const BACKDROP = 'fade-in fixed inset-0 z-50 flex items-start justify-center bg-bg/72 pt-20';
const FRAME = 'card stack w-full max-w-160 bg-overlay shadow-lg';

/* LISTA MA SUFIT WYSOKOŚCI I PRZEWIJA SIĘ SAMA. Bez tego biblioteka z czterdziestoma workflow
   rozpycha okno poza dolną krawędź ekranu, a `body` w tym repo ma `overflow: hidden` — więc
   pozycje spod krawędzi stają się nieosiągalne dla myszy i nie ma jak tego zauważyć inaczej niż
   w przeglądarce (zmierzone 2026-08-31 w chromium).

   448 px to DWANAŚCIE wierszy po 32 px z odstępami po 4 — czyli o jeden więcej, niż liczy
   najdłuższa lista skrótów przy siedmiu sekcjach w rejestrze. Sufit 384 px ucinał jej ostatni
   wiersz i zamieniał „krótką listę" w listę do przewijania: obietnica z `./shortcuts.ts` musi
   się mieścić, a nie tylko być prawdziwa w liczbie wierszy. Strzałki chodzą po CAŁEJ liście
   rzeczy do zrobienia, także poniżej tego sufitu. */
const LIST = 'stack max-h-112 overflow-y-auto';

/** Jak nazywa się rodzaj pozycji w kolumnie po prawej. Jedno słowo, nie zdanie. */
const KIND_SAYS: Readonly<Record<PaletteItem['kind'], string>> = {
  section: 'Section',
  workflow: 'Workflow',
  agent: 'Agent',
};

/** Identyfikator pozycji dla `aria-activedescendant`. Jedno miejsce, żeby oba końce się zgadzały. */
function optionId(index: number): string {
  return 'palette-option-' + String(index);
}

/* Wszystko, na czym `Tab` może stanąć wewnątrz okna. Selektor, a nie lista referencji: liczba
 * kontrolek zależy od tego, która lista stoi w oknie, więc zapisana ręcznie rozjechałaby się
 * przy pierwszej dołożonej. */
const CAN_TAKE_FOCUS = 'input, button, [href], select, textarea, [tabindex]:not([tabindex="-1"])';

export function Palette({
  showing,
  typed,
  items,
  rows,
  at,
  unread,
  onType,
  onStep,
  onChoose,
  onShow,
  onClose,
}: PaletteProps): ReactElement {
  const frame = useRef<HTMLDivElement | null>(null);

  const trap = (event: Typed<HTMLDivElement>): void => {
    const inside = frame.current;
    if (inside === null) return;
    const stops = [...inside.querySelectorAll<HTMLElement>(CAN_TAKE_FOCUS)];
    const first = stops[0];
    const last = stops[stops.length - 1];
    if (first === undefined || last === undefined) return;
    /* Jedna kontrolka znaczy, że nie ma dokąd chodzić — i to też jest pułapka, tyle że pustą
     * pętlą. `preventDefault` niżej i tak nie wypuszcza ogniska poza okno. */
    const goingBack = event.shiftKey;
    const standingOn = document.activeElement;
    if (goingBack && standingOn === first) {
      event.preventDefault();
      last.focus();
      return;
    }
    if (!goingBack && standingOn === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const keys = (event: Typed<HTMLDivElement>): void => {
    if (event.key === 'Tab') {
      trap(event);
      return;
    }
    const next = insideMove({
      key: event.key,
      metaKey: event.metaKey,
      ctrlKey: event.ctrlKey,
      altKey: event.altKey,
      shiftKey: event.shiftKey,
    });
    if (next.move === 'none') return;
    /* Strzałka w otwartej palecie ma chodzić po liście, a nie przewijać ekranu pod spodem;
     * Enter nie ma wysyłać formularza, w którym akurat stało ognisko przed otwarciem. */
    event.preventDefault();
    if (next.move === 'close') {
      onClose();
      return;
    }
    if (next.move === 'step') {
      onStep(next.by);
      return;
    }
    const picked = items[at];
    if (showing === 'items' && picked !== undefined) onChoose(picked);
  };

  return (
    /* Klik w tło zamyka — ale tylko klik W TŁO. `event.target === event.currentTarget` odróżnia
       je od kliknięcia w okno, które w tle się BĄBELKUJE. `onMouseDown`, nie `onClick`: przy
       zaznaczaniu tekstu w polu palec puszcza się często poza oknem, a `click` liczy się wtedy
       od tła i okno zamykałoby się w środku zaznaczania. */
    <div
      className={BACKDROP}
      data-palette-backdrop
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={frame}
        role="dialog"
        aria-modal="true"
        aria-label="Go to anything"
        data-palette={showing}
        className={FRAME}
        data-gap="2"
        onKeyDown={keys}
      >
        <input
          type="text"
          className="field"
          value={typed}
          autoFocus
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={showing === 'items' ? optionId(at) : undefined}
          aria-label="Type to narrow this list"
          placeholder="Jump to a section, run a workflow, open an agent"
          onChange={(event) => {
            onType(event.target.value);
          }}
        />

        {unread ? (
          /* Odmowa dysku jest POWIEDZIANA, nie przemilczana. Paleta z samymi sekcjami wygląda
             identycznie jak paleta w pustej bibliotece, a to są dwie różne prawdy o świecie. */
          <p className="lead" data-palette-unread>
            Loadout could not read what you have saved, so only sections are listed.
          </p>
        ) : null}

        {showing === 'items' ? (
          <ul id="palette-list" role="listbox" aria-label="Things to do" className={LIST}>
            {items.map((item, index) => (
              <li
                key={keyOf(item)}
                id={optionId(index)}
                role="option"
                aria-selected={index === at}
                data-palette-item={item.kind}
                className="row"
                /* `onMouseDown`, nie `onClick`: `click` przychodzi po tym, jak przeglądarka
                   przestawi ognisko na klikniętą pozycję, a wtedy pole traci je w chwili,
                   w której paleta jeszcze stoi. */
                onMouseDown={(event) => {
                  event.preventDefault();
                  onChoose(item);
                }}
              >
                <span className="text-ink">{item.label}</span>
                {item.kind === 'section' && item.letter !== null ? (
                  <span className="label ml-auto">{'G ' + item.letter}</span>
                ) : (
                  <span className="label ml-auto">{KIND_SAYS[item.kind]}</span>
                )}
              </li>
            ))}
          </ul>
        ) : (
          <ul id="palette-list" aria-label="Keyboard shortcuts" className={LIST}>
            {rows.map((row) => (
              <li key={row.press} data-shortcut={row.press} className="row">
                <span className="value">{row.press}</span>
                <span className="lead ml-auto">{row.does}</span>
              </li>
            ))}
          </ul>
        )}

        {(showing === 'items' ? items.length : rows.length) === 0 ? (
          <p className="lead" data-palette-empty>
            Nothing here goes by that name.
          </p>
        ) : null}

        <button
          type="button"
          /* `self-start`, żeby ta kontrolka nie czytała się jak kolejny wiersz listy: wiersze
             są pełnej szerokości i chodzą po nich strzałki, a ten przycisk stoi poza tą pętlą. */
          className="btn-quiet self-start"
          data-palette-show={showing === 'items' ? 'shortcuts' : 'items'}
          onClick={() => {
            onShow(showing === 'items' ? 'shortcuts' : 'items');
          }}
        >
          {showing === 'items' ? 'Keyboard shortcuts' : 'Back to the list'}
        </button>
      </div>
    </div>
  );
}
