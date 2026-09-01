/* Jedno `＋ Add`, a pod nim lista pogrupowana wedle TEGO, PO CO CZŁOWIEK SIĘGA.
 *
 * ── CO TU BYŁO ŹLE, zgłoszenie właściciela 2026-08-31 ───────────────────────────────────────
 *
 * Dolny rząd płótna miał SZEŚĆ przycisków tej samej wagi i tego samego kształtu:
 *   `＋ Add step` · `＋ Add a checkpoint` · `＋ Start something` · `＋ Run a check` ·
 *   `＋ Add loop` · `Tidy up`
 * To są TRZY różne czynności udające jedną rodzinę. Cztery pierwsze stawiają KAFELEK, piąty
 * rysuje STRZAŁKĘ, szósty przestawia CAŁY UKŁAD. Rząd sześciu jednakowych przycisków każe
 * przeczytać wszystkie sześć, żeby znaleźć ten jeden, po który się przyszło — a im dłużej ta
 * rodzina rosła, tym bardziej płótno czytało się jak lista słów kluczowych języka, w którym
 * ktoś ma teraz coś napisać, a nie jak tablica, na której się układa pracę.
 *
 * ── DLACZEGO NAZWY SĄ TAKIE, A NIE INNE ─────────────────────────────────────────────────────
 *
 * Nazwa pozycji mówi, CO POWSTANIE, a nie jak nazywa się wariant w kodzie. Każda ze starych
 * czterech była nazwana z drugiej strony — od mechanizmu:
 *
 *   `＋ Add step` nie mówiło rzeczy najważniejszej: że ten krok robi AGENT. To jest cała jego
 *   treść i jedyny powód, dla którego ten produkt istnieje. Dziś: `A step an agent does`.
 *
 *   `＋ Start something` nie mówiło ani że powstaje KROK, ani co ten krok robi — a kafelek,
 *   który po nim zostawał, nazywał się „Start and leave running", czyli mówił coś innego niż
 *   przycisk, który go postawił. Dziś: `A step that leaves a command running`, tymi samymi
 *   słowami, którymi mówi o sobie kafelek (`tile.tsx`, podpis `leaves it running`).
 *
 *   `＋ Run a check` brzmiało jak POLECENIE („uruchom sprawdzenie teraz"), a stawia kafelek,
 *   który sprawdzi coś dopiero w biegu. Dziś: `A step that runs a check`.
 *
 *   `＋ Add a checkpoint` niosło słowo, które nie mówi niczego o tym, co się stanie. Kafelek
 *   mówi o sobie `asks you` i to jest cała prawda o nim. Dziś: `A step that asks you`.
 *
 *   `＋ Add loop` było TRZECIĄ nazwą jednego mechanizmu: panel kroku nazywa go „Try again up
 *   to", kafelek na strzałce „up to 3 tries", a przycisk „loop". Trzy nazwy jednej rzeczy to
 *   trzy rzeczy dla kogoś, kto widzi ją pierwszy raz. Dziś: `A way back, to try again` — te
 *   same dwa słowa, którymi mówią o niej panel i strzałka.
 *
 * Cztery zaczynają się od `A step`, a piąta NIE — i to jest niesione przez samo brzmienie:
 * powrót nie stawia kafelka, tylko rysuje strzałkę między dwoma, które już stoją.
 *
 * ── DLACZEGO GRUPY, I DLACZEGO TAKIE ────────────────────────────────────────────────────────
 *
 * Grupa nazywa CEL, nie kształt. „Kroki" kontra „strzałki" byłoby pogrupowaniem wedle typu
 * w kodzie — czyli tą samą wadą, przeniesioną o piętro wyżej. Człowiek nie przychodzi po
 * „krok", tylko po „chcę, żeby ktoś to zrobił", „chcę wiedzieć, czy wyszło" i „a jeżeli nie
 * wyszło". Nagłówek `When a check says no` jest z tej trójki najważniejszy: mówi nie tylko,
 * co ta pozycja robi, ale KIEDY po nią sięgnąć — a to była jedyna rzecz, której o powrocie
 * nie mówiło nic w całym produkcie.
 *
 * `Tidy up` NIE MA TU SWOJEJ POZYCJI i to jest rozstrzygnięcie, nie przeoczenie: nie dodaje
 * niczego do grafu, tylko przestawia to, co już stoi. Zostaje osobną kontrolką w `canvas.tsx`.
 *
 * ── DLACZEGO OSOBNY PLIK ────────────────────────────────────────────────────────────────────
 *
 * Ta lista jest CZYSTA — dostaje `open` i dwa handlery, nie zna dokumentu i nie ma stanu. Dzięki
 * temu daje się wyrenderować bez React Flow, a więc i sprawdzić zdanie po zdaniu w środowisku
 * `node`, w którym biegną kryteria tego katalogu. Że ktoś ją naprawdę montuje, dowodzi
 * `e2e/tests/the-canvas-reads-as-a-board.spec.ts` prawdziwym kliknięciem (niezmiennik 29:
 * mechanizm bez montującego jest dokładnie tą klasą wady, dla której to repo powstało).
 */
import type { ReactElement } from 'react';
import type { Step } from '../../../state/workflows';

/** Co można postawić: cztery rodzaje kafelka i powrót, który jest strzałką, nie kafelkiem. */
export type AddChoice = Step['kind'] | 'way-back';

interface AddPick {
  readonly choice: AddChoice;
  /** Zdanie, które czyta człowiek. Mówi, co powstanie. */
  readonly says: string;
}

interface AddGroup {
  /** Po co się po to sięga. Nigdy nazwa kształtu. */
  readonly goal: string;
  readonly picks: readonly AddPick[];
}

/** Lista, słowo w słowo. Eksportowana, bo sądzi ją kryterium obok — powody w nagłówku. */
export const ADD_MENU: readonly AddGroup[] = [
  {
    goal: 'Getting work done',
    picks: [
      { choice: 'agent', says: 'A step an agent does' },
      { choice: 'serve', says: 'A step that leaves a command running' },
    ],
  },
  {
    goal: 'Checking the work',
    picks: [
      { choice: 'check', says: 'A step that runs a check' },
      { choice: 'checkpoint', says: 'A step that asks you' },
    ],
  },
  {
    goal: 'When a check says no',
    picks: [{ choice: 'way-back', says: 'A way back, to try again' }],
  },
];

export interface AddMenuProps {
  /** Czy lista stoi otwarta. Stan mieszka w płótnie, bo płótno zamyka ją także kliknięciem w tło. */
  readonly open: boolean;
  /** Otwiera i zamyka. Bez tego `＋ Add` byłby kontrolką bez skutku (niezmiennik 16). */
  readonly onToggle: () => void;
  /** Wybrano pozycję. Zamknięcie listy należy do wołającego — razem z tym, co ta pozycja robi. */
  readonly onPick: (choice: AddChoice) => void;
  /** Escape. Tryb bez wyjścia jest pułapką, a lista nad płótnem zasłania to, po co się ją otwiera. */
  readonly onDismiss: () => void;
}

/* Lista wychodzi W GÓRĘ (`bottom-full`), bo przycisk stoi przy dolnej krawędzi płótna —
 * rozwinięta w dół wyjechałaby za okno. `absolute`, więc nie zabiera ani jednego piksela
 * układu: dolny rząd ma tę samą wysokość otwarty i zamknięty, a płótno nad nim nie drga.
 *
 * `bg-overlay` i cień, bo ta powierzchnia PŁYWA nad treścią, którą człowiek właśnie czyta —
 * a szkło pod tekstem łamie regułę „szkło jest chrome, treść jest papierem" (theme.css).
 *
 * `.enter` jest JEDNYM regionem na jedno zdarzenie (ARCHITECTURE §7): animuje się pojemnik,
 * nigdy pozycje z osobna. Kaskada w menu czyta się jak usterka, bo człowiek czeka na wiersz,
 * w który chce kliknąć. */
const LIST =
  'card enter absolute bottom-full left-0 z-10 mb-2 flex w-72 flex-col bg-overlay shadow-md';

export function AddMenu({ open, onToggle, onPick, onDismiss }: AddMenuProps): ReactElement {
  return (
    <div
      className="relative"
      onKeyDown={(event) => {
        if (event.key === 'Escape') onDismiss();
      }}
    >
      {open ? (
        <div data-add-list role="menu" aria-label="Add to this workflow" className={LIST}>
          {ADD_MENU.map((group) => (
            <div key={group.goal} role="group" aria-label={group.goal}>
              {/* Nagłówek celu. `.label` — przygaszony stopień 11 px, ten sam, którym ta
                  aplikacja podpisuje pola; nadoczko z wersalikami byłoby drugim krojem
                  w kontrolce, która ma pięć wierszy. */}
              <p className="label px-2.5 pt-2 pb-1">{group.goal}</p>
              {group.picks.map((pick) => (
                <button
                  key={pick.choice}
                  type="button"
                  role="menuitem"
                  data-add-choice={pick.choice}
                  className="row"
                  onClick={() => {
                    onPick(pick.choice);
                  }}
                >
                  {pick.says}
                </button>
              ))}
            </div>
          ))}
        </div>
      ) : null}
      <button
        type="button"
        data-add-open
        aria-haspopup="menu"
        aria-expanded={open ? 'true' : 'false'}
        className="btn"
        onClick={onToggle}
      >
        ＋ Add
      </button>
    </div>
  );
}
