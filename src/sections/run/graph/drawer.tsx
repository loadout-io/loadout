/* Szuflada pod obrazem planu: co powiedział TEN krok, bez zdejmowania z oczu pozostałych.
 *
 * PO CO ISTNIEJE, ZMIERZONE. Do 2026-08-31 jedyną drogą z kafelka do pracy jednego kroku był
 * ekran, który ZAKRYWA całe okno (`../session/`). Odpowiedź na pytanie „co robi ten kafelek"
 * kosztowała więc utratę z oczu wszystkich pozostałych — a bieg równoległy jest zwykłym biegiem
 * (niezmiennik 11) i ogląda się go w całości. DESIGN §7 wymienia wysuwany strumień kroku wprost,
 * jako jedną z trzech powierzchni, które POJAWIAJĄ SIĘ nad tym, co już jest na ekranie.
 *
 * TO NIE JEST DRUGI EKRAN AGENTA, i różnica jest w tym, na co każde z nich odpowiada. Szuflada
 * odpowiada na „co ten krok mówi TERAZ", w miejscu, w które człowiek właśnie patrzy, i nie
 * zabiera obrazu. Ekran agenta odpowiada na „co ten agent dostał i co zostawił" — dwa bloki
 * FAKTÓW z dysku, których w strumieniu nie ma — i na to potrzebuje całego okna. Droga tam
 * prowadzi stąd, jednym przyciskiem: rzecz tania od razu, droga o jedno kliknięcie dalej.
 *
 * WIERSZ JEST TEN SAM, CO W STRUMIENIU GŁÓWNYM (`../feed/line.tsx`), a filtr jest tą samą
 * funkcją, z której żyje ekran agenta (`../session/filter.ts`). Drugi renderer wiersza albo
 * druga derywacja pokazywałyby przy tej samej linii inny podział na grupy albo inne rozwinięcie,
 * a nic na ekranie nie mówiłoby, który z dwóch obrazów jest prawdziwy (niezmiennik 23).
 *
 * ESCAPE ZAMYKA, i to jest jedyny nasłuch klawiatury w tym drzewie. Wisi na oknie, nie na
 * szufladzie: kursor stoi w wierszu wejścia przez większość czasu pracy (`../index.tsx`,
 * `caretBackToTheField`), więc nasłuch na samej szufladzie nie usłyszałby ani jednego
 * naciśnięcia. Zdejmowany przy odmontowaniu — szuflady nie ma, więc nie ma czego zamykać,
 * a nasłuch bez powierzchni jest handlerem, który pewnego dnia zamknie coś innego.
 */
import { useEffect } from 'react';
import type { ReactElement } from 'react';

import { Line } from '../feed/line';
import type { FeedView } from '../feed/model';
import { sessionFeed } from '../session/filter';
import type { GraphStep } from './model';

export interface StepStreamProps {
  /** Krok, którego szuflada jest otwarta — ten sam obiekt, z którego powstał jego kafelek. */
  readonly step: GraphStep;
  /** Strumień, z którego wycinamy wiersze tego kroku. Nigdy druga derywacja. */
  readonly view: FeedView;
  /** Rozwinięcie wiersza — ten sam handler, co w strumieniu głównym. */
  readonly onToggle: (rowId: number) => void;
  /** Zamknięcie. Wymagane: powierzchnia bez wyjścia jest pułapką, nie powierzchnią. */
  readonly onClose: () => void;
  /**
   * Wejście w pełny ekran tego agenta — albo brak, kiedy nie wiemy, kto za tym krokiem stoi.
   *
   * Krok, o którym strumień nic nie powiedział, nie ma agenta do pokazania, a przycisk
   * otwierający pusty ekran jest kontrolką bez skutku z dodatkowym krokiem (niezmiennik 16).
   */
  readonly onOpenAgent?: () => void;
}

/* `.btn-quiet` z `theme.css` — jedna definicja cichego przycisku na całą aplikację. */
const QUIET = 'btn-quiet';

/**
 * Zdanie, kiedy ten krok jeszcze nic nie nadał.
 *
 * Mówi o TYM kroku, nie o biegu: strumień obok bywa wtedy pełny, więc „nic tu jeszcze nie ma"
 * bez wskazania czyjego czytałoby się jak awaria okna (DESIGN §6).
 */
export const NOTHING_FROM_THIS_STEP = 'Nothing from this step yet.';

export function StepStream({
  step,
  view,
  onToggle,
  onClose,
  onOpenAgent,
}: StepStreamProps): ReactElement {
  /* Escape jest DRUGĄ drogą do tej samej czynności, nie drugą czynnością: `onClose` jest tą
   * samą funkcją, którą woła przycisk obok, więc nie ma dwóch miejsc, w których szuflada się
   * zamyka (niezmiennik 13). */
  useEffect(() => {
    function shut(event: KeyboardEvent): void {
      if (event.key !== 'Escape') return;
      onClose();
    }
    window.addEventListener('keydown', shut);
    return () => {
      window.removeEventListener('keydown', shut);
    };
  }, [onClose]);

  const said = sessionFeed(view, step.name);

  return (
    /* `.enter`: szuflada POJAWIA SIĘ po kliknięciu w kafelek, więc wchodzi sprężyną — jedno
       z trzech miejsc, w których DESIGN §7 na nią pozwala, i JEDNORAZOWO (`both`). Obraz nad
       nią nie rusza się ani o piksel.

       `shrink-0` plus sufit wysokości: szuflada bierze tyle, ile ma treści, i nigdy więcej niż
       połowę kolumny. Bez sufitu długi krok wypycha obraz planu do zera — czyli zabiera to,
       obok czego miał stanąć. */
    <div
      data-step-stream={step.id}
      className="enter flex max-h-1/2 min-h-0 shrink-0 flex-col border-t border-line bg-panel"
    >
      <div className="flex shrink-0 items-center gap-2 px-[14px] py-[7px]">
        {/* Nazwa kroku, tym samym stopniem, co nadoczko kolumny — szuflada należy do kafelka,
            a nie jest osobnym ekranem, więc nie dostaje własnego tytułu. */}
        <h3 className="min-w-0 flex-1 truncate font-mono text-eyebrow text-muted" title={step.name}>
          {step.name}
        </h3>
        {onOpenAgent === undefined ? null : (
          <button type="button" onClick={onOpenAgent} className={QUIET}>
            Open this agent
          </button>
        )}
        <button type="button" onClick={onClose} className={QUIET}>
          Close
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {said.length === 0 ? (
          <p data-empty className="lead px-[18px] pb-2">
            {NOTHING_FROM_THIS_STEP}
          </p>
        ) : (
          said.map((row) => <Line key={row.id} row={row} onToggle={onToggle} />)
        )}
      </div>
    </div>
  );
}
