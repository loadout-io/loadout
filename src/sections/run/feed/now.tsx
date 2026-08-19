/* Strefa TERAZ — stała wysokość, nadpisywana w miejscu. Jak `top`, nie jak `tail -f` [DESIGN §1].
 *
 * Jeden agent, jeden wiersz, przepisywany. Wycinek historii (`lines.slice(-4)`) wygląda na
 * zrzucie ekranu identycznie i pełznie o wiersz na każde zdarzenie — dlatego wiersze przychodzą
 * z modelu kluczowane agentem, a ten plik nie ma prawa ich wybierać ani obcinać.
 *
 * `Thinking…` jest JEDNYM wierszem na całą strefę, bez nazwy agenta [T2 §7.2 wiersz 4]. Model
 * wie, czyj slot jest żywy, i ta wiedza przyda się szynie agentów (T-09); tutaj wystarczy, że
 * ktoś myśli — druga kopia tej samej informacji obok wiersza agenta byłaby drugim żywym
 * regionem na jeden fakt (niezmiennik 13).
 *
 * Bez animacji. DESIGN §7 mówi wprost: zmiana treści w tej strefie nie animuje się, bo oko
 * goni wtedy ruch zamiast czytać. Sufit „regiony animujące się od jednego zdarzenia" wynosi 2
 * [ARCHITECTURE §7]; ta strefa nie wydaje z niego ani jednego.
 */
import type { ReactElement } from 'react';
import type { NowZone } from './model';

export interface NowProps {
  now: NowZone;
  /**
   * Czy bieg NAPRAWDE zyje.
   *
   * Bramkowanie zywego regionu sama liczba wierszy bylo defektem znalezionym przez druga opinie:
   * `doing` w `feed/model.ts` jest tylko DOPISYWANE i nigdy nie czyszczone, wiec po zakonczeniu
   * biegu strefa dalej trzymala trzy wiersze („waiting on Forge") i dalej pulsowala coralem —
   * czyli mowila „dzieje sie" o czyms, co stoi, i wydawala jeden z dwoch regionow animujacych
   * z ARCHITECTURE §7 na fakt falszywy przez wiekszosc czasu.
   *
   * Ten fakt mieszka w wywolujacym i tylko tam (niezmiennik 13).
   */
  live: boolean;
}

export function Now({ now, live }: NowProps): ReactElement {
  return (
    <div data-now className="glass shrink-0 rounded-md py-2">
      {/* JEDEN ZYWY REGION NA JEDEN FAKT (niezmiennik 13, limit 1). Fakt brzmi „cos sie teraz
          dzieje" — nie „ten konkretny agent pisze", bo tego danych nie ma: `NowRow` to
          `{ agent, text }`, a kto pracuje, a kto czeka, jest trescia zdania. Wyprowadzanie tego
          w widoku przez szukanie slowa `waiting` wymyslilo by fakt (niezmiennik 17) i postawilo
          polityke „kto co robi" drugi raz, w komponencie (niezmiennik 23).

          Kropka pulsuje i jest coralowa — jest jednym z dwoch regionow, ktorym ARCHITECTURE §7
          pozwala sie ruszac, i jedyna rzecza w tej strefie, ktora niesie barwe. Nie ma jej wcale,
          kiedy nie ma wierszy: coral, ktory swieci przy pustej strefie, przestaje cokolwiek
          znaczyc. */}
      {live && now.rows.length > 0 ? (
        <div className="flex items-center gap-[7px] px-4 pb-1">
          <span aria-hidden className="size-1.5 animate-blip rounded-pill bg-live" />
          <span className="text-eyebrow text-muted">Now</span>
        </div>
      ) : null}

      {now.rows.map((row) => (
        /* Siatka `stream-line` z DESIGN §6: `88px 1fr auto`. Trzecia kolumna zostaje pusta —
           czas trwania mieszka na pasku loadoutu i nigdzie indziej (niezmiennik 13). */
        <div key={row.agent} className="grid grid-cols-[88px_1fr_auto] gap-2 px-4 py-2">
          <span className="font-mono text-mono-strong text-muted">{row.agent}</span>
          <span className="text-stream text-ink">{row.text}</span>
        </div>
      ))}

      {now.thinking === null ? null : <p className="px-4 py-2 text-stream text-muted">Thinking…</p>}
    </div>
  );
}
