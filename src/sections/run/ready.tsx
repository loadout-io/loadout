/* CO STOI W KOLUMNIE STRUMIENIA, ZANIM PADNIE PIERWSZA LINIA — a setup jest już kompletny.
 *
 * ZMIERZONE 2026-08-31, na zrzucie prawdziwego okna 1512×950 (`e2e/harness.ts`, chromium).
 * Człowiek, który ma już folder, agenta i workflow, dostawał na najważniejszym ekranie
 * aplikacji jeden znak `◇` i zdanie „Nothing here yet: the work shows up line by line." —
 * czyli KOMUNIKAT O BRAKU DANYCH, którego `docs/design/DESIGN.md` §6 zabrania wprost
 * („Pusty ekran to zaproszenie do działania, nie komunikat o braku danych"). Pod nim było
 * 760 px czerni na 1010 px szerokości: największa rzecz na ekranie nie niosła niczego.
 *
 * CO WCHODZI NA TO MIEJSCE I DLACZEGO AKURAT TO. Ostatni bieg TEGO folderu. Dwa powody, oba
 * mierzalne:
 *
 *   1. Kolumna strumienia jest miejscem na TRANSKRYPT. Zapis ostatniego biegu jest
 *      transkryptem — jedyną treścią, która do tej kolumny naprawdę należy, kiedy nowej
 *      jeszcze nie ma. Wszystko inne, co dałoby się tu postawić, byłoby ozdobą wstawioną po
 *      to, żeby zapełnić (DESIGN §1 nazywa to wprost jako rzecz, której się nie robi).
 *   2. Jest to rzecz, po którą człowiek na ten ekran WRACA. Model widoku mówi to samo
 *      z drugiej strony: szuflada kroku zostaje po zejściu biegu, „bo transkrypt biegu, który
 *      właśnie zszedł, jest jedyną rzeczą, po którą człowiek na ten ekran wraca"
 *      (`./feed/model.ts`, akapit przy `runEnded`).
 *
 * KARTA NIE JEST DRUGĄ HISTORIĄ. Panel `/history` pokazuje WSZYSTKIE biegi i jest widokiem
 * na całe okno; ta karta pokazuje JEDEN i jest zaproszeniem do niego. Wejście prowadzi do tego
 * samego panelu, tą samą funkcją (`../history-command.ts`, `openOneRun`), więc nie powstaje
 * druga droga do zapisanego biegu (niezmiennik 13). Słowa o stanie i o koszcie też są stamtąd
 * — `stateWord` i `costText` — bo wiersz historii mówiący „done" i karta mówiąca o tym samym
 * biegu inaczej czytają się jak dwa różne biegi.
 *
 * ANI JEDNEJ CZYNNOŚCI GŁÓWNEJ. Na ekranie Run czynność główna jest jedna i jest nią `Run`
 * w pasku loadoutu (DESIGN §1: jedna na ekran). Wejście w ostatni bieg jest czynnością ZWYKŁĄ,
 * więc `.btn` — obrys, nie akcent. Drugi akcent w tym samym kadrze znaczyłby, że nikt nie
 * rozstrzygnął, po co człowiek tu przyszedł.
 *
 * DLACZEGO NIE `justify-center`. Bo transkrypt rośnie OD GÓRY. Karta wyśrodkowana w pionie
 * stanęłaby dokładnie tam, gdzie za chwilę nie będzie jej miejsca, a odstęp pod nią przestaje
 * być pustką w chwili, w której pada pierwsza linia — jest miejscem trzymanym dla rzeczy,
 * która właśnie ma przyjść.
 */
import type { ReactElement } from 'react';

import { costText, stateWord } from './history-command';
import type { PastRunRow } from './io';
import { stepsText } from './past/panel';

/**
 * Zaproszenie, kiedy w tym folderze nie biegło jeszcze nic.
 *
 * JEDNO ZDANIE W TRYBIE ROZKAZUJĄCYM, i nazywa kontrolkę, która ten ruch wykonuje. Zdanie
 * `NOTHING_YET` z `../history-command.ts` odpowiada na INNE pytanie — człowiek napisał
 * `/history` i pyta, co tu biegło — więc odsyła do `/run`, czyli do drogi, którą właśnie
 * szedł. Tutaj nikt o nic nie pytał, a kontrolka startu stoi w kadrze: odesłanie do komendy
 * zamiast do widocznego przycisku byłoby dłuższą drogą do tej samej rzeczy.
 *
 * EKSPORTOWANE, żeby kryterium mogło je CZYTAĆ, a nie przepisywać — napis przepisany do testu
 * przestaje pilnować czegokolwiek w dniu, w którym ktoś zmieni brzmienie i nie tknie kryterium.
 */
export const NOTHING_RAN_HERE_YET = 'Press Run to start the first one in this folder.';

/** Nadoczko karty. Mówi, o którym biegu jest — nigdy o tym, ile ich było. */
export const LAST_RUN = 'Last run';

/** Napis wejścia w zapisany bieg. Czasownik, bo to jest czynność, a nie nazwa miejsca. */
export const OPEN_LAST_RUN = 'Open it';

/**
 * Druga linia karty: stan, ile kroków, ile kosztował, kiedy.
 *
 * FUNKCJA CZYSTA, poza komponentem, bo to repo nie ma jsdom — a to jest jedyne miejsce, gdzie
 * z czterech faktów powstaje jedno zdanie, i regresja gubiąca którykolwiek z nich musi mieć
 * czym paść.
 *
 * CZŁON, KTÓREGO NIE ZNAMY, NIE WCHODZI (niezmiennik 17). `costText` oddaje pusty napis, kiedy
 * żaden krok nie podał kosztu — a `$0.00` w tym miejscu mówiłoby „nie kosztowało nic" o biegu,
 * którego nikt nie zmierzył. Tak samo słowo z drutu, którego nie umiemy przełożyć: `stateWord`
 * oddaje wtedy pustkę, zamiast wypuścić na ekran enum (niezmiennik 14).
 */
export function lastRunFacts(row: PastRunRow): string {
  return [stateWord(row.state), stepsText(row.steps), costText(row.costUsd), row.when]
    .filter((part) => part !== '')
    .join(' · ');
}

export interface ReadyToRunProps {
  /** Ostatni bieg tego folderu, albo `null`, kiedy nie biegło tu jeszcze nic. */
  readonly lastRun: PastRunRow | null;
  /** Otwiera ten bieg do odczytu. Bez niego karta nie ma dokąd wpuścić i nie rysuje wejścia. */
  readonly onOpenLastRun: () => void;
}

export function ReadyToRun({ lastRun, onOpenLastRun }: ReadyToRunProps): ReactElement {
  return (
    <div
      data-run-ready
      className="flex min-h-0 flex-1 flex-col items-start gap-3 overflow-y-auto px-[18px] py-3"
    >
      {lastRun === null ? (
        /* Znacznik pustego ekranu siedzi na SAMYM zdaniu — `src/sections/empty-screen-invites
           .test.tsx` czyta jego treść i żąda jednego zdania bez glifu i bez przycisku obok
           w środku. */
        <p data-empty className="lead">
          {NOTHING_RAN_HERE_YET}
        </p>
      ) : (
        /* `.card` z `theme.css`: pojemnik listy, ten sam, którym rysuje się wiersz historii
           (`./past/panel.tsx`). Szerokość TREŚCI, nie kolumny — karta rozciągnięta na 1010 px
           byłaby paskiem, a nie rzeczą, w którą się wchodzi. */
        <div data-last-run={lastRun.folder} className="card grid max-w-lg gap-2">
          {/* Nadoczko, więc stopień `text-eyebrow` — on jeden nosi wersaliki (DESIGN §4). */}
          <h2 className="font-mono text-eyebrow text-muted">{LAST_RUN}</h2>
          {/* Jak workflow nazwał SAM SIEBIE. Kiedy Rust nie dał rady przeczytać opisu, zostaje
              data — pusty nagłówek byłby kartą bez tożsamości. Ta sama zamiana, co w panelu
              historii (`./past/panel.tsx`, `PastRunView`). */}
          <p className="text-heading text-ink">
            {lastRun.title === '' ? lastRun.when : lastRun.title}
          </p>
          <p className="value">{lastRunFacts(lastRun)}</p>
          {/* Czynność ZWYKŁA: obrys, nie akcent — powód w nagłówku pliku. */}
          <button type="button" className="btn justify-self-start" onClick={onOpenLastRun}>
            {OPEN_LAST_RUN}
          </button>
        </div>
      )}
    </div>
  );
}
