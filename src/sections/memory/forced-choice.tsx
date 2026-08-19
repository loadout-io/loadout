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
import type { Choice, Note } from '../../state/memory';
import { lengthLabel } from './note-row';

export interface ForcedChoiceProps {
  choice: Choice;
  /** Notatki, które sekcja trzyma — po to, żeby lista mówiła zdaniami, a nie nazwami plików. */
  notes: Note[];
  /** „Stop using" na pozycji z listy. Kontrolka bez handlera nie wchodzi do repo (16). */
  onStopUsing: (id: string) => void;
  /** Zamknięcie bez zgody na cokolwiek. */
  onCancel: () => void;
}

/* `modal` z DESIGN §6: tło `--panel`, obrys `--line-strong`, szerokość do 640px, padding 24px.
 * Tło za oknem to `--bg` przy 72% — z tokena, nie z zapisanego wprost `rgba(6,9,11,0.72)`,
 * bo ta sama liczba w dwóch miejscach jest tym, jak paleta przestaje być zamknięta. */
const BACKDROP = 'fixed inset-0 flex items-center justify-center bg-bg/72';
const WINDOW =
  'flex w-full max-w-160 flex-col gap-3 rounded-lg border border-line-strong bg-panel p-6';
const ACT = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

/** Zdanie o tym, ile brakuje. Jedna liczba, jedno miejsce — resztę mówi lista pod nim. */
function overBySentence(overBy: number): string {
  return 'This note is ' + String(overBy) + ' longer than the room that is left.';
}

export function ForcedChoice({
  choice,
  notes,
  onStopUsing,
  onCancel,
}: ForcedChoiceProps): ReactElement {
  return (
    <div className={BACKDROP}>
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="memory-is-full"
        data-choice={choice.id}
        className={WINDOW}
      >
        <h2 id="memory-is-full" className="text-heading text-ink">
          Memory is full
        </h2>
        <p className="text-body text-body">{overBySentence(choice.overBy)}</p>

        <ul className="flex flex-col gap-2">
          {choice.retire.map((id) => {
            /* Notatka, o której sekcja nic nie wie, zostaje na liście pod swoją nazwą pliku:
               odmowa liczy się z WSZYSTKICH plików zakresu, a sekcja trzyma tylko to, co
               akurat pokazuje. Wycięcie takiej pozycji zabrałoby człowiekowi z listy właśnie
               tę, której odstawienie zwolniłoby najwięcej. */
            const note = notes.find((one) => one.id === id);
            return (
              <li key={id} className="flex items-center justify-between gap-2">
                <span className="text-body text-ink">{note?.rule ?? id}</span>
                <span className="text-label text-muted">
                  {note === undefined ? '' : lengthLabel(note.length)}
                </span>
                <button
                  type="button"
                  data-stop={id}
                  className={ACT}
                  onClick={() => {
                    onStopUsing(id);
                  }}
                >
                  Stop using
                </button>
              </li>
            );
          })}
        </ul>

        <div className="flex items-center justify-end gap-2">
          <button type="button" data-cancel className={ACT} onClick={onCancel}>
            Not now
          </button>
        </div>
      </section>
    </div>
  );
}
