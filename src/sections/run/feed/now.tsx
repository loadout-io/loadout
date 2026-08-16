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
}

export function Now({ now }: NowProps): ReactElement {
  return (
    <div data-now className="shrink-0 rounded-sq bg-panel py-2">
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
