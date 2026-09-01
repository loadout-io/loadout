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
import { useState } from 'react';
import type { Fix, Note } from '../../../state/workflows';
import type { FileAccess } from '../../../state/agents';

export interface RunBarProps {
  /** Uwagi z ostatniego sprawdzenia. Pusta lista znaczy „nie ma nic do poprawienia". */
  notes: Note[];
  onRun: () => void;
  /** Kliknięcie uwagi przesuwa płótno na winny krok i otwiera jego panel. */
  onFocusNote: (note: Note) => void;
  /** Wykonuje naprawę, którą niesie uwaga. Brak propsu znaczy „ten ekran nie umie naprawiać". */
  onApplyFix?: (fix: Fix) => void;
}

/** Co [`focusNote`] woła. Obie funkcje przychodzą z płótna — `fitView` z `useReactFlow()`,
 * `openPanel` z ekranu — więc sama funkcja nie potrzebuje ani okna, ani hooka. */
export interface NoteFocus {
  fitView: (options: { nodes: Array<{ id: string }>; duration: number; maxZoom: number }) => void;
  openPanel: (stepId: string) => void;
}

/* 2026-08-31 — `RUN`, `RUN_OFF` I `FIX` ZNIKŁY. Pierwsze dwa były jednym przyciskiem opisanym
 * dwa razy: stała rysowała szary przycisk ręcznie obok wypełnionego, a warunek wybierający
 * jedną z nich stał tuż obok atrybutu `disabled`, który mówi dokładnie to samo. Stan wyłączony
 * jest od dziś REGUŁĄ (`.btn-primary:disabled` w `theme.css`), więc odpowiedź „czy da się
 * uruchomić" ma w markupie jedno miejsce: sam atrybut.
 *
 * Wariantu `disabled:` Tailwinda tu dalej NIE MA i ten powód nie wygasł: zostawiłby słowo
 * `disabled` w atrybucie `class` także pod DZIAŁAJĄCYM przyciskiem, a kryterium tej sekcji
 * czyta obecność tego słowa w atrybutach jako odmowę startu.
 *
 * `FIX` był przyciskiem cichym z DESIGN §6 przepisanym ręcznie i o 4 px niższym niż wszystkie
 * pozostałe — piąta wysokość przycisku w repo. Teraz to `.btn-quiet`. */

/** Uwagi, które Loadout umie naprawić sam. */
function fixable(notes: Note[]): Note[] {
  return notes.filter((note) => note.fix !== undefined);
}

/** Napis na przycisku mówi, CO SIĘ STANIE, a nie „Fix" — człowiek ma wiedzieć, na co się zgadza.
 *
 * Dial nazywa pozycję, na którą przechodzi, bo to jest zmiana uprawnień i nie ma prawa odbyć się
 * pod ogólnikiem. Lista narzędzi nazywa agenta, bo naprawa dotyczy roli używanej też gdzie
 * indziej — a to jest jedyna różnica między tymi dwiema naprawami, którą człowiek musi widzieć
 * PRZED kliknięciem. */
function fixLabel(fix: Fix): string {
  if (fix.kind === 'widenFileAccess') return `Set this step to ${DIAL[fix.to]}`;
  if (fix.kind === 'giveItAFreshCopy') return 'Give it its own copy';
  return `Take them off ${fix.agentName}`;
}

/** Trzy pozycje dialu tak, jak brzmią w formularzu agenta (`agents/agent-form.tsx`).
 *
 * Kotwicą są tamte trzy napisy, nie ta stała: człowiek czyta je na ekranie agenta, a to zdanie
 * widzi dopiero na przycisku naprawy. Ten sam podział i ten sam powód, co przy `on_screen`
 * w Ruście. */
const DIAL: Record<FileAccess, string> = {
  'look-only': 'Look only',
  'ask-first': 'Ask first',
  'work-freely': 'Work freely',
};

/** Kropka wagi: problem świeci kolorem awarii, ostrzeżenie kolorem „wymaga ciebie". */
const DOT: Record<Note['level'], string> = {
  problem: 'text-fail',
  warning: 'text-attend',
};

/** Ile uwag widać, zanim człowiek poprosi o resztę.
 *
 * 2026-08-23 — ZGŁOSZENIE WŁAŚCICIELA: „ogarnij ten UI z errorami bo mi zalewa ekran". Ten pasek
 * rysował KAŻDĄ uwagę, a jedna reguła — dwa kroki bez strzałki w jednym folderze — zgłasza się
 * PER PARĘ, więc dziesięć nienazwanych kafelków daje czterdzieści pięć zdań. Płótna spod nich
 * nie było widać, a Run stał na dole listy.
 *
 * Trzy, a nie pięć czy dziesięć: tyle mieści się nad przyciskiem bez spychania go z ekranu przy
 * najniższym oknie, jakie ten produkt obsługuje. */
const AT_FIRST = 3;

/** Problemy przed ostrzeżeniami — bo tylko problem blokuje Run.
 *
 * Kiedy widać trzy z czterdziestu, to MUSZĄ być te trzy, które zatrzymują bieg. Lista w kolejności
 * walidatora pokazywałaby czasem trzy ostrzeżenia i chowała pod „pokaż wszystkie" jedyną rzecz,
 * przez którą nic nie rusza.
 *
 * `toSorted`, nie `sort`: `notes` przychodzi propsem i posortowanie go w miejscu zmieniałoby
 * tablicę należącą do wołającego. Sortowanie jest STABILNE, więc uwagi tej samej wagi zostają
 * w kolejności, w której zgłosił je walidator — a to jest kolejność kroków w pliku. */
function worstFirst(notes: Note[]): Note[] {
  return notes.toSorted(
    (one, other) => Number(other.level === 'problem') - Number(one.level === 'problem'),
  );
}

/** „2 things to fix" — jedno zdanie, policzone z listy uwag i z niczego innego.
 *
 * Liczba pojedyncza nie jest kosmetyką: „1 things to fix" czyta się jak usterka narzędzia,
 * a użytkownik ma w tej chwili wierzyć, że narzędzie wie, co mówi. */
function howMany(notes: Note[]): string {
  return `${String(notes.length)} ${notes.length === 1 ? 'thing' : 'things'} to fix`;
}

export function RunBar({ notes, onRun, onFocusNote, onApplyFix }: RunBarProps): ReactElement {
  /* Zwinięte na starcie i przy KAŻDYM nowym sprawdzeniu — nie zapamiętujemy rozwinięcia między
   * dokumentami. Człowiek, który rozwinął czterdzieści uwag w jednym workflow i przeszedł do
   * drugiego, dostawałby tam czterdzieści cudzych. */
  const [showAll, setShowAll] = useState(false);
  const sorted = worstFirst(notes);
  const shown = showAll ? sorted : sorted.slice(0, AT_FIRST);
  const hidden = sorted.length - shown.length;
  /* Blokuje WYŁĄCZNIE `problem`. Pasek, który liczy wszystkie uwagi i przy każdej gasi Run,
   * zamienia ostrzeżenie o niepodłączonym kroku w zamek bez klucza — a taki workflow wolno
   * uruchomić. Podpowiedź jest samą uwagą, słowo w słowo z walidatora: „Fix the errors first"
   * pod zgaszonym przyciskiem mówi użytkownikowi, że nie może kliknąć, i nic poza tym. */
  const blocker = notes.find((note) => note.level === 'problem');

  return (
    <div className="flex flex-col gap-2">
      {notes.length > 0 ? (
        <div className="flex flex-col gap-1">
          <span className="label">{howMany(notes)}</span>
          {/* KLUCZ Z POZYCJI, NIE Z TREŚCI — naprawa z 2026-08-23, z konsoli okna właściciela:
              „Encountered two children with the same key, `warning:s_3:"New step" and "New step"
              can run at the same time…`". Walidator zgłasza tę regułę PER PARĘ kroków, a zdanie
              nazywa je po nazwie i kropkuje na pierwszym z pary — więc trzy nienazwane kafelki
              („New step") dają trzy uwagi identyczne co do bajtu. React ostrzega wprost, że przy
              zdublowanym kluczu może wiersz POMINĄĆ, a uwaga, której nie widać, jest gorsza od
              tej, która się powtarza.

              Uwagi przychodzą z jednego wywołania walidatora i lista nie jest sortowana ani
              filtrowana w miejscu, więc pozycja jest tu stabilna między renderami. */}
          {shown.map((note, at) => (
            <div
              key={`${String(at)}:${note.level}:${note.stepId ?? ''}`}
              className="flex items-baseline gap-2"
            >
              {/* WIERSZ, KTÓRY REAGUJE. To zdanie przesuwa płótno i otwiera panel winnego kroku,
                  a do 2026-08-31 nie robiło pod kursorem nic — czyli czytało się jak napis.
                  `w-auto` znosi `width:100%` prymitywu, bo przycisk naprawy stoi OBOK, w tym
                  samym wierszu, i ma zostać widoczny. */}
              <button
                type="button"
                className="row w-auto items-baseline text-body text-ink"
                onClick={() => {
                  onFocusNote(note);
                }}
              >
                <span className={DOT[note.level]}>●</span>
                {note.message}
              </button>
              {/* PRZYCISK ISTNIEJE TYLKO TAM, GDZIE NAPRAWA JEST JEDNOZNACZNA (niezmiennik 16:
                  kontrolka bez skutku nie wchodzi do repo). Uwagę o kształcie grafu naprawia się
                  przeciągnięciem strzałki i takiej uwagi ten przycisk nie dostaje. */}
              {note.fix === undefined || onApplyFix === undefined ? null : (
                <button
                  type="button"
                  data-fix
                  className="btn-quiet"
                  onClick={() => {
                    onApplyFix(note.fix as Fix);
                  }}
                >
                  {fixLabel(note.fix)}
                </button>
              )}
            </div>
          ))}
          {/* RESZTA NA ŻĄDANIE. Liczba stoi na przycisku, bo „Show all" nie mówi, na ile się
              zgadzasz — a przy czterdziestu uwagach to jest różnica między rzutem oka a stroną
              tekstu. Wracając, przycisk mówi, do ilu wraca, żeby ta droga nie była jednokierunkowa. */}
          {hidden > 0 || showAll ? (
            <button
              type="button"
              data-show-all-notes
              className="label self-start text-left underline hover:text-ink"
              onClick={() => {
                setShowAll(!showAll);
              }}
            >
              {showAll ? `Show fewer` : `Show ${String(hidden)} more`}
            </button>
          ) : null}

          {/* Jedno kliknięcie na wszystkie, kiedy jest co zbierać. Pięć naprawialnych uwag to
              dziś pięć kliknięć, a każda z nich jest tą samą decyzją: „zrób to, co i tak bym
              zrobił ręcznie". */}
          {onApplyFix === undefined || fixable(notes).length < 2 ? null : (
            <button
              type="button"
              data-fix-all
              className="btn-quiet self-start"
              onClick={() => {
                for (const note of fixable(notes)) onApplyFix(note.fix as Fix);
              }}
            >
              {`Fix all ${String(fixable(notes).length)}`}
            </button>
          )}
        </div>
      ) : null}

      <button
        type="button"
        className="btn-primary"
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
