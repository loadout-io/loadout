/* Kopiowanie allowlistowanej diagnostyki aktywnego workspace — T-34 AC-4.
 *
 * RAPORT NIGDY NIE WRACA DO OKNA. Rust składa go i zapisuje przez plugin schowka; ten komponent
 * dostaje wyłącznie licznikowy paragon, którego treści nawet nie renderuje. Dzięki temu prompt,
 * stderr albo absolutna ścieżka nie mogą przeciec przez przypadkowy log stanu Reacta.
 *
 * ODMOWA JEST STAŁYM ZDANIEM. Surowy błąd może sam zawierać prywatny materiał z czytania
 * artefaktów, więc `why(error, …)` byłoby tutaj naruszeniem granicy, nie lepszą diagnostyką.
 */
import type { ReactElement } from 'react';
import { useRef, useState } from 'react';

import { copyDiagnostics } from './io';

export const DIAGNOSTICS_COPIED = 'Diagnostics copied';
export const DIAGNOSTICS_FAILED = 'Loadout could not copy diagnostics.';

export interface DiagnosticsProps {
  /** Folder aktywnego workspace albo `null`, gdy człowiek nie wskazał jeszcze miejsca pracy. */
  readonly folder: string | null;
}

type Result = 'idle' | 'copying' | 'copied' | 'failed';

interface ViewIdentity {
  readonly folder: string | null;
  readonly generation: number;
}

interface Status {
  readonly owner: ViewIdentity;
  readonly result: Exclude<Result, 'idle'>;
}

export function Diagnostics({ folder }: DiagnosticsProps): ReactElement {
  const identity = useRef<ViewIdentity>({ folder, generation: 0 });
  if (identity.current.folder !== folder) {
    identity.current = { folder, generation: identity.current.generation + 1 };
  }
  const here = identity.current;
  const [status, setStatus] = useState<Status | null>(null);
  /* Ref jest zapadką natychmiastową. Dwa kliki w tym samym tyknięciu widzą ją, zanim React
   * zdąży przerysować `disabled`, więc jedna prośba człowieka nie kopiuje raportu dwa razy.
   * Niesie TOŻSAMOŚĆ widoku, nie bool: kopia A w toku nie blokuje B, a jej późny wynik nie ma
   * prawa przestawić zdania stojącego już przy innym folderze. */
  const copying = useRef<ViewIdentity | null>(null);

  function copy(): void {
    if (here.folder === null || copying.current === here) return;
    const request = here;
    copying.current = request;
    setStatus({ owner: request, result: 'copying' });
    void copyDiagnostics(request.folder)
      .then(() => {
        if (identity.current === request) setStatus({ owner: request, result: 'copied' });
      })
      .catch(() => {
        /* Nigdy nie pokazuj wartości odrzuconej obietnicy — powód w nagłówku pliku. */
        if (identity.current === request) setStatus({ owner: request, result: 'failed' });
      })
      .finally(() => {
        if (copying.current === request) copying.current = null;
      });
  }

  /* Zmiana folderu zeruje ekran SYNCHRONICZNIE w renderze. Efekt byłby o klatkę za późno i
   * mignąłby człowiekowi „Diagnostics copied" z poprzedniego projektu pod nazwą nowego. */
  const result: Result = status?.owner === here ? status.result : 'idle';

  const sentence =
    result === 'copied' ? DIAGNOSTICS_COPIED : result === 'failed' ? DIAGNOSTICS_FAILED : null;

  return (
    <div data-diagnostics className="flex shrink-0 items-center gap-2">
      {sentence === null ? null : (
        <p data-diagnostics-said aria-live="polite" className="lead fade-in min-w-0 truncate">
          {sentence}
        </p>
      )}
      <button
        type="button"
        aria-label="Copy diagnostics"
        title={
          folder === null
            ? 'Choose a workspace before copying diagnostics.'
            : 'Copy a private-safe support summary for this workspace.'
        }
        disabled={here.folder === null || result === 'copying'}
        onClick={copy}
        /* `.btn-quiet` z `theme.css` — DEMOCJA Z 2026-08-31, nie kosmetyka.
           Recznie spisana wersja stala tu kiedys na 28 px; `.btn` naprawil rozmiar i zostawil
           WAGE: kopiowanie diagnostyki wygladalo w rzedzie paska dokladnie tak samo, jak wybor
           lidera i pole zadania, i stalo przed nimi wszystkimi. To jest czynnosc RZADKA — sięga
           po nia czlowiek, ktory zglasza usterke — a rzad kontrolek jednej wagi znaczy, ze nikt
           nie rozstrzygnal, co jest wazne (DESIGN §1: trzy poziomy glosnosci). Na tym ekranie
           czynnosc glowna jest jedna i jest nia `Run`.
           `shrink-0`, bo pasek nie sciska juz rzedu na sile (`./strip/strip.tsx`): ustepuja
           napisy, nigdy kontrolki. */
        className="btn-quiet shrink-0"
      >
        {result === 'copying' ? 'Copying…' : 'Copy diagnostics'}
      </button>
    </div>
  );
}
