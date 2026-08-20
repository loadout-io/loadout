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
import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';
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
const SECONDARY = 'h-8 rounded-sm border border-line-strong bg-raised px-3 text-ui text-ink';

/** `button-quiet` z DESIGN §6. */
const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

interface AskedProps {
  question: Question;
  onAnswer: (questionId: number, option: string) => void;
}

/** Zachęta pola odpowiedzi. Zdanie, nie opis stanu (DESIGN §6). */
export const ANSWER_PROMPT = 'Type your answer and press Enter';

/**
 * Pytanie do człowieka, przyklejone [T2 §7.2 wiersz 10].
 *
 * Kolor `--attend` odpowiada na jedno pytanie: co czeka na MOJĄ decyzję (DESIGN §3). Opcje
 * przychodzą z linii, nigdy stąd: pytanie z opcjami dopisanymi w widoku odpowiada agentowi coś,
 * czego nie pytał.
 *
 * POLE TEKSTOWE JEST ZAWSZE, I TO JEST NAPRAWA, NIE OZDOBA. Zmierzone 2026-08-18: Rust wysyła
 * `options: Vec::new()` w KAŻDYM punkcie kontrolnym (`commands::run::ask`), a ten blok rysował
 * wyłącznie przyciski z tej listy — czyli kartę „Needs your answer" z ZEREM kontrolek. Każdy
 * workflow z kafelkiem punktu kontrolnego był przez to nieukończalny: pytanie stało na ekranie
 * i nie było czym na nie odpowiedzieć. Przyciski zostają tam, gdzie opcje naprawdę są: wybór
 * z trzech jest szybszy niż przepisywanie jednej z nich ręcznie.
 *
 * GDZIE TA TREŚĆ JEDZIE, i to jest druga połowa naprawy. `answer()` stawia ją w `view.toCarry`,
 * czyli w kolejce wysyłkowej o pojemności jednego zdania, a zabiera ją stąd kontrolka „dalej":
 * `continue_run` bierze po tamtej stronie `answer: Option<String>` (`src-tauri/src/ipc.rs`)
 * i podaje je agentowi razem z podbiciem licznika zgód. Ten blok nie woła komendy sam i nie ma
 * prawa: bieg puszcza JEDNA kontrolka w całej aplikacji, a druga byłaby drugim miejscem, z
 * którego da się odblokować bieg — pierwszy rozjazd między nimi jest cichy (niezmiennik 13).
 */
function Asked({ question, onAnswer }: AskedProps): ReactElement {
  const [typed, setTyped] = useState('');

  function send(event: FormEvent<HTMLFormElement>): void {
    /* Bez tego przeglądarka przeładowuje stronę i bieg znika razem z nią — okno Tauri nie ma
     * dokąd nawigować, a magazyny żyją na poziomie modułu. */
    event.preventDefault();
    /* Puste Enter nie jest odpowiedzią. Wysłane, zdjęłoby pytanie z ekranu i zostawiło bieg
     * stojący na czymś, o czym okno już nie mówi. */
    if (typed.trim() === '') return;
    onAnswer(question.id, typed.trim());
    setTyped('');
  }

  return (
    <div className="shrink-0 rounded-md border border-attend-edge border-l-2 border-l-attend bg-attend-soft p-3">
      <p className="text-label text-attend">Needs your answer</p>
      <p className="mt-1 text-body text-ink">{question.text}</p>

      {question.options.length === 0 ? null : (
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
      )}

      <form onSubmit={send} className="mt-2 flex items-center gap-2">
        <input
          aria-label="Your answer"
          placeholder={ANSWER_PROMPT}
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
          className="field flex-1"
        />
        <button type="submit" className={SECONDARY}>
          Send
        </button>
      </form>
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
          <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
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
            {/* KOMENDA JEDZIE Z WIERSZA, i to jest cała droga propozycji do przycisku: model
                przepisuje ją z linii, ten plik podaje ją komponentowi, a `line.tsx` rysuje
                kontrolkę wyłącznie wtedy, gdy ją dostanie. Bez tej jednej właściwości przycisk
                startu istnieje tylko w teście — czyli jest kontrolką, której nikt nie zobaczy
                (niezmiennik 16). O tym, CZY on w ogóle jest, rozstrzyga rodzaj wiersza, czyli
                decyzja podjęta w Ruście; ta linia niczego nie rozpoznaje. */}
            {view.history.map((row) => (
              <Line key={row.id} row={row} onToggle={onToggle} command={row.command} />
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
