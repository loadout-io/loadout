/* Pasek nad przyciskiem Run: ile jest rzeczy do poprawienia i czy Run w ogóle działa.
 *
 * Uwagi przychodzą Z RUSTA (`workflow::check`, T-12) i są tu tylko wyświetlane. Frontend ich nie
 * wymyśla, nie tłumaczy i nie liczy po swojemu — `message` jest gotowym angielskim zdaniem
 * i to ono ląduje w `title` zablokowanego przycisku. Zablokowany Run z podpowiedzią
 * „Fix the errors first" jest przyciskiem bez wyjaśnienia: użytkownik widzi, że nie może
 * kliknąć, i nie wie dlaczego [T3 §5.3].
 *
 * Podział wagi jest całą treścią tego paska: `Problem` blokuje Run, `Warning` NIE blokuje.
 * Pasek, który liczy wszystkie uwagi i przy każdej gasi Run, zamienia ostrzeżenie o niepodłączonym
 * kroku w blokadę uruchomienia — a to jest workflow, który wolno uruchomić.
 */
import type { ReactElement } from 'react';
import type { Note } from '../../../state/workflows';

export interface RunBarProps {
  /** Uwagi z ostatniego sprawdzenia. Pusta lista znaczy „nie ma nic do poprawienia". */
  notes: Note[];
  onRun: () => void;
  /** Kliknięcie uwagi przesuwa płótno na winny krok i otwiera jego panel. */
  onFocusNote: (note: Note) => void;
}

/** Co [`focusNote`] woła. Obie funkcje przychodzą z płótna — `fitView` z `useReactFlow()`,
 * `openPanel` z ekranu — więc sama funkcja nie potrzebuje ani okna, ani hooka. */
export interface NoteFocus {
  fitView: (options: { nodes: Array<{ id: string }>; duration: number; maxZoom: number }) => void;
  openPanel: (stepId: string) => void;
}

/* Przycisk podstawowy (DESIGN §6 `button-primary`) i jego wersja bez mocy sprawczej. Klasa jest
 * wybierana TUTAJ, a nie wariantem `disabled:` Tailwinda: wariant zostawiłby słowo `disabled`
 * w atrybucie `class` także wtedy, gdy przycisk działa, więc „czy da się uruchomić" miałoby
 * w HTML-u dwie odpowiedzi, z których jedna kłamie (niezmiennik 13). */
const RUN = 'h-9 rounded-sq bg-accent px-4 text-ui text-bg';
const RUN_OFF = 'h-9 rounded-sq bg-raised px-4 text-ui text-muted';

/** Kropka wagi: problem świeci kolorem awarii, ostrzeżenie kolorem „wymaga ciebie". */
const DOT: Record<Note['level'], string> = {
  problem: 'text-fail',
  warning: 'text-attend',
};

/** „2 things to fix" — jedno zdanie, policzone z listy uwag i z niczego innego.
 *
 * Liczba pojedyncza nie jest kosmetyką: „1 things to fix" czyta się jak usterka narzędzia,
 * a użytkownik ma w tej chwili wierzyć, że narzędzie wie, co mówi. */
function howMany(notes: Note[]): string {
  return `${String(notes.length)} ${notes.length === 1 ? 'thing' : 'things'} to fix`;
}

export function RunBar({ notes, onRun, onFocusNote }: RunBarProps): ReactElement {
  /* Blokuje WYŁĄCZNIE `problem`. Pasek, który liczy wszystkie uwagi i przy każdej gasi Run,
   * zamienia ostrzeżenie o niepodłączonym kroku w zamek bez klucza — a taki workflow wolno
   * uruchomić. Podpowiedź jest samą uwagą, słowo w słowo z walidatora: „Fix the errors first"
   * pod zgaszonym przyciskiem mówi użytkownikowi, że nie może kliknąć, i nic poza tym. */
  const blocker = notes.find((note) => note.level === 'problem');

  return (
    <div className="flex flex-col gap-2">
      {notes.length > 0 ? (
        <div className="flex flex-col gap-1">
          <span className="text-label text-muted">{howMany(notes)}</span>
          {notes.map((note) => (
            <button
              key={`${note.level}:${note.stepId ?? ''}:${note.message}`}
              type="button"
              className="flex items-baseline gap-2 text-left text-body text-ink"
              onClick={() => {
                onFocusNote(note);
              }}
            >
              <span className={DOT[note.level]}>●</span>
              {note.message}
            </button>
          ))}
        </div>
      ) : null}

      <button
        type="button"
        className={blocker === undefined ? RUN : RUN_OFF}
        disabled={blocker !== undefined}
        title={blocker?.message}
        onClick={onRun}
      >
        Run
      </button>
    </div>
  );
}

/** Przesuwa płótno na krok, którego dotyczy uwaga, i otwiera jego panel.
 *
 * Uwaga bez `stepId` dotyczy całego pliku i nie ma w co celować — wtedy nie dzieje się nic. */
export function focusNote(note: Note, focus: NoteFocus): void {
  /* Uwaga o całym pliku („There are no steps yet.") nie ma w co celować. Przesunięcie płótna
   * gdziekolwiek byłoby wtedy ruchem bez powodu, a otwarty panel — panelem cudzego kroku. */
  if (note.stepId === null) return;

  /* 400 ms i sufit powiększenia 1.2 są z T3 §5.3, oba zmierzone na `FitViewOptions`: bez sufitu
   * pojedynczy kafelek rozjeżdża się na cały ekran i użytkownik traci z oczu resztę grafu. */
  focus.fitView({ nodes: [{ id: note.stepId }], duration: 400, maxZoom: 1.2 });
  focus.openPanel(note.stepId);
}
