/* Okno wymuszonego wyboru: zakres jest pełny, więc człowiek decyduje, co z niego wychodzi.
 *
 * DLACZEGO TO OKNO W OGÓLE ISTNIEJE. „When a promotion would exceed the cap, Loadout does not
 * silently trim — it shows a forced choice" [T6 §5.3]. Ciche przycięcie wygląda w interfejsie
 * identycznie jak sukces i różni się tylko tym, że notatka, którą człowiek zatwierdził,
 * przestaje docierać do modelu. Tego się nie da zauważyć z ekranu — ani wtedy, ani nigdy.
 *
 * KOLEJNOŚĆ LISTY JEST JEJ TREŚCIĄ. Przychodzi z odmowy, najdawniej użyte pierwsze, i nie jest
 * tu ani razu przeliczana: sekcja nie zna `last_used_at` wszystkich plików, więc posortowana
 * tutaj byłaby drugą odpowiedzią na to samo pytanie, liczoną z połowy danych (niezmiennik 13).
 *
 * KAŻDA POZYCJA NIESIE SWOJĄ DŁUGOŚĆ, bo bez niej wybór jest zgadywaniem: człowiek ma dobrać
 * to, co pokryje brakującą liczbę ze zdania wyżej, a nie klikać w pierwszą pozycję z brzegu.
 *
 * Czysta funkcja propsów na markup, jak `NoteRow`: bez własnego stanu i bez `invoke()`. Okno
 * otwiera i zamyka magazyn (`src/state/memory.ts`) — komponent, który zamykałby się sam,
 * miałby drugie zdanie o tym, czy wybór jest jeszcze otwarty.
 */
import type { ReactElement } from 'react';
import type { Choice, Note, NoteAddress } from '../../state/memory';
import { lengthLabel } from './note-row';

export interface ForcedChoiceProps {
  choice: Choice;
  /** Notatki, które sekcja trzyma — po to, żeby lista mówiła zdaniami, a nie nazwami plików. */
  notes: Note[];
  /** „Stop using" na pozycji z listy. Kontrolka bez handlera nie wchodzi do repo (16). */
  onStopUsing: (address: NoteAddress) => void;
  /** Zamknięcie bez zgody na cokolwiek. */
  onCancel: () => void;
}

/* `modal` z DESIGN §6: tło `--panel`, obrys `--line-strong`, szerokość do 640px, padding 24px.
 * Tło za oknem to `--bg` przy 72% — z tokena, nie z zapisanego wprost `rgba(6,9,11,0.72)`,
 * bo ta sama liczba w dwóch miejscach jest tym, jak paleta przestaje być zamknięta. */
/* WEJŚCIE JEST SAMĄ PRZEZROCZYSTOŚCIĄ, i to jest reguła, nie oszczędność. DESIGN §6 mówi
 * o modalu wprost: „bez rozmycia, bez animacji wjazdu poza `opacity`" — sprężyna należy do
 * powierzchni, które WCHODZĄ w widok, a okno wymuszonego wyboru zasłania go w całości.
 * `.fade-in` niesie tę jedną obietnicę i stoi na CAŁYM przyciemnieniu, więc jedno zdarzenie
 * porusza tu jednym regionem (sufit z ARCHITECTURE §7 wynosi dwa). */
const BACKDROP = 'fade-in fixed inset-0 flex items-center justify-center bg-bg/72';
const WINDOW =
  'flex w-full max-w-160 flex-col gap-3 rounded-lg border border-line-strong bg-overlay p-6';

/** Zdanie o tym, ile brakuje. Jedna liczba, jedno miejsce — resztę mówi lista pod nim. */
function overBySentence(overBy: number): string {
  return 'This note is ' + String(overBy) + ' longer than the room that is left.';
}

/**
 * Co się stanie po odstawieniu — i o którą notatkę tu chodzi (2026-08-31).
 *
 * DWIE RZECZY, KTÓRYCH TO OKNO NIE MÓWIŁO. Nie nazywało notatki, po którą człowiek przyszedł
 * (jej treść stała wyłącznie w atrybucie `data-choice`), i nie mówiło, że odstawienie DOMYKA
 * tamtą prośbę — a to jest jedyny powód, dla którego ktokolwiek miałby cokolwiek odstawiać.
 * Człowiek czytał listę cudzych zdań i nie miał jak zgadnąć, że po kliknięciu dostanie to,
 * po co przyszedł. Jedno zdanie mówi obie połowy: następny ruch i jego skutek.
 *
 * Notatka nieznana sekcji wypada z cudzysłowu zamiast wjechać w niego nazwą pliku: slug
 * w zdaniu dla człowieka jest gorszy niż jego brak (niezmiennik 14).
 */
function nextMoveSentence(rule: string | undefined): string {
  const what = rule === undefined ? 'the note you picked' : JSON.stringify(rule);
  return 'Stop using one of these, and Loadout will put ' + what + ' to use right away.';
}

export function ForcedChoice({
  choice,
  notes,
  onStopUsing,
  onCancel,
}: ForcedChoiceProps): ReactElement {
  /* Notatka, o którą człowiek poprosił. Szukana po CAŁYM adresie, bo sam `id` może legalnie
   * wystąpić w obu korzeniach (`NoteAddress` w `src/state/memory.ts`). */
  const wanted = notes.find(
    (one) => one.place === choice.address.place && one.id === choice.address.id,
  );

  return (
    <div className={BACKDROP}>
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="memory-is-full"
        data-choice={`${choice.address.place}:${choice.address.id}`}
        className={WINDOW}
      >
        <h2 id="memory-is-full" className="text-heading text-ink">
          Memory is full
        </h2>
        <p>{overBySentence(choice.overBy)}</p>
        {/* Następny ruch i jego skutek — patrz `nextMoveSentence`. Stopień i barwa idą z `.lead`:
            to jest zdanie drugoplanowe pod zdaniem o brakującej liczbie, nie drugi nagłówek. */}
        <p className="lead">{nextMoveSentence(wanted?.rule)}</p>

        <ul className="flex flex-col gap-2">
          {choice.retire.map((id) => {
            /* Notatka, o której sekcja nic nie wie, zostaje na liście pod swoją nazwą pliku:
               odmowa liczy się z WSZYSTKICH plików zakresu, a sekcja trzyma tylko to, co
               akurat pokazuje. Wycięcie takiej pozycji zabrałoby człowiekowi z listy właśnie
               tę, której odstawienie zwolniłoby najwięcej. */
            const note = notes.find((one) => one.place === choice.address.place && one.id === id);
            const address: NoteAddress = { place: choice.address.place, id };
            return (
              <li
                key={`${address.place}:${address.id}`}
                className="flex items-center justify-between gap-2"
              >
                <span className="text-ink">{note?.rule ?? id}</span>
                <span className="label">{note === undefined ? '' : lengthLabel(note.length)}</span>
                <button
                  type="button"
                  data-stop={id}
                  className="btn-quiet"
                  onClick={() => {
                    onStopUsing(address);
                  }}
                >
                  Stop using
                </button>
              </li>
            );
          })}
        </ul>

        <div className="flex items-center justify-end gap-2">
          <button type="button" data-cancel className="btn-quiet" onClick={onCancel}>
            Not now
          </button>
        </div>
      </section>
    </div>
  );
}
