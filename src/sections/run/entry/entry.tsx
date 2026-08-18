/* Wiersz wejścia widoku pracy (makieta `docs/mockup/index.html`, reguła `.entry`).
 *
 * CO TEN WIERSZ ROBI I DLACZEGO TYLE. Makieta obiecuje tu `/plan · /run · or just say what you
 * want`, czyli parser języka naturalnego i planistę. Planisty w tym repo nie ma, a wiersz,
 * który przyjmuje zdanie i odpowiada „jeszcze tego nie umiem", jest gorszy od jego braku:
 * obiecuje sposób pracy, którego nie ma (niezmiennik 16), i robi to przy KAŻDYM naciśnięciu
 * Enter. Dlatego zachęta wymienia dokładnie te komendy, które ten wiersz naprawdę wykonuje,
 * a kryterium AC-4 czyta ją z markupu i sprawdza, że każde wymienione słowo jest rozumiane —
 * dopisanie `/plan` do zachęty zapala test, zanim zobaczy je człowiek.
 *
 * DLACZEGO TE DWIE, A NIE `/run`. Uruchomienie biegu bierze DWIE rzeczy: workflow i limit „ile
 * naraz". Limit mieszka w suwaku obok przycisku Start (`limits/at-once.tsx`, stan `start.tsx`),
 * więc `/run` wpisane tutaj musiałoby wybrać limit po swojemu — i cicho zignorować to, co
 * człowiek przed chwilą ustawił suwakiem. Dwa miejsca, w których powstaje ta sama liczba, to
 * niezmiennik 13 złamany w najgorszym możliwym miejscu: w argumencie, który decyduje, ilu
 * agentów naprawdę ruszy. `/run` wchodzi tu w dniu, w którym limit ma jedno miejsce.
 *
 * TO NIE JEST DRUGA ŚCIEŻKA DO TYCH CZYNNOŚCI, TYLKO SKRÓT DO TYCH SAMYCH FUNKCJI. `/open` woła
 * dokładnie ten handler, który wisi pod `＋` na pasku kart, a `/stop` ten, który wisi pod Stop.
 * Ekran pracy podaje oba propsem — gdyby ten plik wołał `io.ts` sam, nazwa komendy istniałaby
 * w sekcji dwa razy (niezmiennik 23).
 *
 * ZERO ŻARGONU W TEKŚCIE WIDOCZNYM (niezmiennik 14, DESIGN §8): „folder", „run", „stop" —
 * żadnego `workspace`, `session`, `process`, `execute`.
 */
import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';

/**
 * Komendy, które ten wiersz wykonuje — cała lista, w kolejności zachęty.
 *
 * Zamknięta jako WARTOŚĆ, nie jako zdanie w komentarzu: zachęta i odpowiedź „nie znam tego"
 * są z niej składane, więc nie da się dopisać komendy do napisu, nie ucząc jej wiersza.
 */
export const COMMANDS = ['/open', '/stop'] as const;

export type Command = (typeof COMMANDS)[number];

/** Co człowiek widzi w pustym polu — zachęta, nie opis stanu (DESIGN §6). */
export const PROMPT = '/open a folder  ·  /stop the run';

/** Druga linia z makiety (`.entry .hint`): co robi Enter i jak daleko sięga ten wiersz. */
export const HINT = 'Enter runs it. These two are everything this line understands so far.';

/** Odpowiedź na `/stop`, kiedy nic nie biegnie. Cisza czyta się jak zepsuty klawisz. */
export const NOTHING_RUNS = 'Nothing is running.';

/** Odpowiedź na słowo, którego ten wiersz nie zna — z listą, która JEST listą, nie kopią. */
export const NOT_KNOWN =
  'That one is not known here. This line takes ' + COMMANDS.join(' and ') + '.';

/**
 * Komenda, którą niesie ta linia — albo `null`.
 *
 * Rozstrzyga PIERWSZE słowo, nie całe zdanie: `/open ~/Projects/x` ma otworzyć wybór folderu,
 * a nie odbić się od nierozpoznanej linii. Reszty wiersz dziś nie czyta i nie udaje, że czyta
 * — ścieżki wpisanej z palca nie ma czym sprawdzić, a karta otwarta na folder, którego nie ma,
 * jest kłamstwem o dysku (niezmiennik 4).
 */
export function understand(typed: string): Command | null {
  const first = typed.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
  return COMMANDS.find((command) => command === first) ?? null;
}

export interface EntryProps {
  /** Wymagany: wybór folderu — ten sam handler, co pod `＋` na pasku kart (niezmiennik 16). */
  readonly onOpenFolder: () => void;
  /**
   * Zatrzymanie biegu, albo `null`, kiedy nic nie biegnie.
   *
   * `null`, a nie osobne pole `running`: „czy jest co zatrzymywać" i „czym to zatrzymać" to
   * jeden fakt, a dwa pola obok siebie dają stan, w którym mówią co innego.
   */
  readonly onStopRun: (() => void) | null;
}

export function Entry({ onOpenFolder, onStopRun }: EntryProps): ReactElement {
  const [typed, setTyped] = useState('');
  /** Ostatnia odpowiedź wiersza; `null`, dopóki nie ma o czym mówić. */
  const [said, setSaid] = useState<string | null>(null);

  function send(event: FormEvent<HTMLFormElement>): void {
    /* Bez tego przeglądarka przeładowuje stronę i bieg znika razem z nią — okno Tauri nie ma
     * dokąd nawigować, a magazyny żyją na poziomie modułu. */
    event.preventDefault();
    if (typed.trim() === '') return;

    const command = understand(typed);
    setTyped('');

    if (command === '/open') {
      setSaid(null);
      onOpenFolder();
      return;
    }
    if (command === '/stop') {
      if (onStopRun === null) {
        setSaid(NOTHING_RUNS);
        return;
      }
      setSaid(null);
      onStopRun();
      return;
    }
    setSaid(NOT_KNOWN);
  }

  return (
    <form
      data-entry
      onSubmit={send}
      className="border-t border-line-strong px-[18px] pt-[10px] pb-3"
    >
      <div className="grid h-10 grid-cols-[26px_1fr_auto] items-center border border-line-strong border-l-2 border-l-accent bg-well">
        {/* Znak zachęty z makiety. `aria-hidden`, bo dla czytnika ekranu to jest ozdoba. */}
        <span aria-hidden className="text-center font-mono text-accent">
          ❯
        </span>
        <input
          aria-label="Command line"
          placeholder={PROMPT}
          spellCheck={false}
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
          className="h-[38px] border-0 bg-transparent font-mono text-mono text-ink outline-0"
        />
        <kbd className="mr-[9px] border border-line px-[5px] py-[2px] font-mono text-label text-muted">
          ENTER
        </kbd>
      </div>

      <p data-entry-hint className="mt-[6px] ml-[26px] font-mono text-label text-muted">
        {HINT}
      </p>

      {said === null ? null : (
        <p data-entry-said className="mt-[6px] ml-[26px] text-body text-attend">
          {said}
        </p>
      )}
    </form>
  );
}
