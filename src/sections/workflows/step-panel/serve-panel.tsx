/* Panel kafelka „uruchom i zostaw" — dwa wiersze, bo ten kafelek ma dwa pola.
 *
 * Istnieje z tego samego powodu, co `checkpoint-panel.tsx`, i ten powód jest niezmiennikiem 16:
 * płótno ma przycisk `＋ Start something`, a przycisk, który stawia kafelek bez sposobu na wpisanie
 * komendy, jest kontrolką prowadzącą donikąd. Kafelek z pustą komendą wygląda na płótnie prawie
 * tak samo jak wypełniony, a odmawia dopiero w środku biegu.
 *
 * To NIE jest `StepPanel` z siódemką wierszy ani panel kroku „sprawdź": tu nie ma agenta, więc
 * nie ma dziedziczenia, nadpisań ani wiersza Skills — i nie ma pola „Proof that it ran", bo ten
 * kafelek niczego nie orzeka. Wspólny formularz z połową wierszy schowanych warunkiem jest tą
 * samą konstrukcją, którą DESIGN §6 nazywa zakładkami w panelu.
 *
 * Ramki tu nie ma: rysuje ją `PanelForStep`, jedną, dla wszystkich paneli.
 */
import type { ReactElement } from 'react';
import type { ServeStep } from '../../../state/workflows';

export interface ServePanelProps {
  step: ServeStep;
  onEditStep: (fields: Partial<Pick<ServeStep, 'name' | 'command' | 'folder'>>) => void;
}

/* GDZIE TO WSTAJE — dwa wyjścia, bo dwa mają dla serwera sens.
 *
 * 2026-08-23 — DOSZŁO PO PIERWSZYM PRAWDZIWYM UŻYCIU. Kafelek wychodził z przycisku z folderem
 * projektu i nie dało się tego zmienić, a to jest dla serwera zły domyślny: sprawdzenie, które
 * ma na niego patrzeć, pracuje w kopii kroku, który właśnie pisał kod — więc serwer z folderu
 * projektu podaje kod BEZ tej pracy. Strona, która się otwiera i pokazuje starą wersję, wygląda
 * na działającą, a to jest gorsze niż serwer, którego nie ma.
 *
 * Własnej kopii tu nie ma i nie powinno być: świeży checkout serwowałby kod, którego nikt w tym
 * biegu nie tknął — czyli ten sam błąd, tylko drożej. */
const WHERE = [
  {
    use: 'same-copy' as const,
    name: 'In the same copy as the step before it',
    why: 'Serves the code that step just wrote. Pick this when a later step has to look at it.',
  },
  {
    use: 'project' as const,
    name: 'In your project folder',
    why: 'Serves what is in your project right now.',
  },
];

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
/* Klasa domu, nie własny opis — ten sam powód, co w `checkpoint-panel.tsx`. */
const FIELD = 'field';

export function ServePanel({ step, onEditStep }: ServePanelProps): ReactElement {
  return (
    <>
      <div className={ROW}>
        <label htmlFor="serve-name" className={LABEL}>
          Name
        </label>
        <input
          id="serve-name"
          className={FIELD}
          value={step.name}
          onChange={(event) => {
            onEditStep({ name: event.target.value });
          }}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="serve-command" className={LABEL}>
          Command to run
        </label>
        <input
          id="serve-command"
          className={FIELD}
          placeholder="npm run dev"
          value={step.command}
          onChange={(event) => {
            onEditStep({ command: event.target.value });
          }}
        />
        {/* To zdanie jest CAŁĄ różnicą między tym kafelkiem a krokiem „sprawdź" i musi stać
            tam, gdzie człowiek podejmuje decyzję (niezmiennik 29). Druga połowa mówi, DOKĄD ta
            rzecz idzie i kiedy umiera: bez niej człowiek nie wie, czy zostanie mu w tle serwer
            trzymający port. „Started" jest nazwą TEJ SEKCJI z ekranu biegu (`rail.tsx`), nie
            naszym słowem — człowiek ma szukać tego, co widzi (niezmiennik 13). */}
        <span className={LABEL}>
          The steps after this one start right away, without waiting for it to finish. It stays
          alive under Started on the right until you stop it there or close Loadout.
        </span>
      </div>

      <div className={ROW}>
        <span className={LABEL}>Where it runs</span>
        {WHERE.map((one) => (
          <label key={one.use} className="flex items-baseline gap-2 text-body text-ink">
            <input
              type="radio"
              name="serve-where"
              value={one.use}
              /* `pick` na tym kafelku nie ma kontrolki, więc plik z taką wartością nie zaznaczy
               * ani jednego pola — i to jest uczciwsze niż zaznaczenie czegoś, czego w pliku nie
               * ma (niezmiennik 17). Człowiek widzi wtedy, że musi wybrać. */
              checked={step.folder.use === one.use}
              onChange={() => {
                onEditStep({ folder: { use: one.use } });
              }}
            />
            <span className="min-w-0">
              {one.name}
              <span className={'block ' + LABEL}>{one.why}</span>
            </span>
          </label>
        ))}
      </div>
    </>
  );
}
