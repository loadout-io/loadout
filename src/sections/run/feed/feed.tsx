/* Strefa HISTORII — jedyna, która przyrasta [DESIGN §1].
 *
 * PRZYPIĘCIE DO DOŁU ROBI UKŁAD, NIE SKRYPT. Kontener jedzie w `flex-col-reverse`, więc treść
 * sama siedzi przy dolnej krawędzi, a najnowszy wiersz stoi pod `scrollTop === 0`. To jest cała
 * implementacja „widok nie wyrywa zdania spod oczu": nie ma `useEffect`, który przewija po
 * paczce, więc nie ma czego wyłączać ani czym warunkować. Jedyne wywołanie portu w tym drzewie
 * wychodzi z przycisku `Jump to newest` — i to jest jedyna droga imperatywna, jaką model ma.
 *
 * `Jump to newest` jest widoczny zawsze, kiedy jest dokąd skakać, i to jest decyzja, nie
 * przeoczenie: warunek „pokaż, gdy użytkownik odjechał od dołu" wymaga ODCZYTU pozycji, a
 * odczyt jest dotknięciem portu — czyli dokładnie tym, czego kryterium 1 zabrania. Kontrolka,
 * która zawsze coś robi, jest tańsza niż kontrolka, która wie, kiedy się pokazać.
 *
 * PYTANIE MA TU JEDNO ŻYWE MIEJSCE. Wiersz `asked` zostaje w historii jako zapis tego, co się
 * wydarzyło, ale przyciski odpowiedzi są WYŁĄCZNIE w bloku przyklejonym — dwa komplety
 * przycisków na to samo pytanie to dwa miejsca, w których bieg da się odblokować, i pierwszy
 * rozjazd między nimi jest cichy (niezmiennik 13).
 */
import type { ReactElement } from 'react';
import { Line } from './line';
import type { FeedView, Question } from './model';

export interface FeedProps {
  view: FeedView;
  /** Element, po którym jeździ port przewijania. Podpina go ekran. */
  portRef: (element: HTMLDivElement | null) => void;
  onToggle: (rowId: number) => void;
  onAnswer: (questionId: number, option: string) => void;
  onJumpToNewest: () => void;
}

/** `button-secondary` z DESIGN §6, spisany raz. */
const SECONDARY = 'h-8 rounded-sq border border-line-strong bg-raised px-3 text-ui text-ink';

/** `button-quiet` z DESIGN §6. */
const QUIET = 'h-7 rounded-sq border border-line px-3 text-ui text-body';

interface AskedProps {
  question: Question;
  onAnswer: (questionId: number, option: string) => void;
}

/**
 * Pytanie do człowieka, przyklejone [T2 §7.2 wiersz 10].
 *
 * Kolor `--attend` odpowiada na jedno pytanie: co czeka na MOJĄ decyzję (DESIGN §3). Opcje
 * przychodzą z linii, nigdy stąd: pytanie narysowane bez swoich opcji jest kontrolką bez
 * handlera, a pytanie z opcjami dopisanymi w widoku odpowiada agentowi coś, czego nie pytał.
 */
function Asked({ question, onAnswer }: AskedProps): ReactElement {
  return (
    <div className="shrink-0 rounded-sq border border-attend-edge bg-attend-wash p-3">
      <p className="text-label text-attend">Needs your answer</p>
      <p className="mt-1 text-body text-ink">{question.text}</p>
      <div className="mt-2 flex flex-wrap gap-2">
        {question.options.map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => onAnswer(question.id, option)}
            className={SECONDARY}
          >
            {option}
          </button>
        ))}
      </div>
    </div>
  );
}

export function Feed({
  view,
  portRef,
  onToggle,
  onAnswer,
  onJumpToNewest,
}: FeedProps): ReactElement {
  /* Nic w historii i nikogo w strefie TERAZ znaczy: biegu jeszcze nie było. Sam pusty strumień
   * przy pracujących agentach to co innego — wtedy zaproszenie kłamałoby o stanie maszyny. */
  const nothingYet = view.history.length === 0 && view.now.rows.length === 0;

  return (
    <section data-feed className="flex min-h-0 flex-1 flex-col gap-2">
      {nothingYet ? (
        /* Pusty ekran to zaproszenie, nie komunikat o braku danych (DESIGN §6) — ale bez
         * przycisku i bez „Type /plan to start", bo wiersz wejścia i paleta poleceń są osobną
         * powierzchnią i w tej wersji ich nie ma. Zaproszenie wskazujące na kontrolkę, której
         * nie ma, jest gorsze niż zdanie mniej (niezmiennik 16). */
        <div className="flex flex-1 flex-col items-center justify-center gap-3">
          <span className="flex size-8 items-center justify-center rounded-sq border border-dashed border-line-strong text-muted">
            ◇
          </span>
          <p data-empty className="text-ink">
            Nothing here yet: the work shows up line by line.
          </p>
        </div>
      ) : (
        <div ref={portRef} className="flex min-h-0 flex-1 flex-col-reverse overflow-y-auto">
          {/* Jedno dziecko kontenera odwróconego: wiersze zostają w swojej kolejności, a to,
              co się odwraca, to kierunek wypełniania — czyli przypięcie do dołu. */}
          <div>
            {view.history.map((row) => (
              <Line key={row.id} row={row} onToggle={onToggle} />
            ))}
          </div>
        </div>
      )}

      {view.pinned === null ? null : <Asked question={view.pinned} onAnswer={onAnswer} />}

      {view.history.length === 0 ? null : (
        <div className="flex shrink-0 justify-end">
          <button type="button" onClick={onJumpToNewest} className={QUIET}>
            Jump to newest
          </button>
        </div>
      )}
    </section>
  );
}
