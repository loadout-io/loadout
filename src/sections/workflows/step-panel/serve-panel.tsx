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
import { fieldNameFor } from './hands-over-the-command';
import { WhereItWorks } from './where-it-works';
import type { ServeStep } from '../../../state/workflows';

export interface ServePanelProps {
  step: ServeStep;
  onEditStep: (
    fields: Partial<Pick<ServeStep, 'name' | 'command' | 'folder' | 'commandFrom'>>,
  ) => void;
  /**
   * Nazwa kroku, który po strzałce stoi przed tym kafelkiem, albo `null`.
   *
   * Liczona przez EDYTOR, nie tutaj: panel nie zna strzałek, więc nie ma jak pomylić się co do
   * tego, który krok jest tym przed (ten sam ruch i ten sam powód, co przy `wayBack`).
   */
  stepBefore?: string | null | undefined;
  /** Czy ten krok jest już proszony o pole, na które ten kafelek czeka. */
  handsItOver?: boolean | undefined;
  /** Człowiek prosi krok przed tym o to pole — jawnym kliknięciem, nie efektem ubocznym. */
  onAskTheStepBefore?: (() => void) | undefined;
}

/* GDZIE TO WSTAJE — DWA wyjścia, bo tylko dwa mają dla serwera sens.
 *
 * 2026-08-23 — WYBÓR DOSZEDŁ PO PIERWSZYM PRAWDZIWYM UŻYCIU. Kafelek wychodził z przycisku
 * z folderem projektu i nie dało się tego zmienić, a to jest dla serwera zły domyślny:
 * sprawdzenie, które ma na niego patrzeć, pracuje w kopii kroku, który właśnie pisał kod — więc
 * serwer z folderu projektu podaje kod BEZ tej pracy. Strona, która się otwiera i pokazuje starą
 * wersję, wygląda na działającą, a to jest gorsze niż serwer, którego nie ma.
 *
 * Własnej kopii tu nie ma i nie powinno być: świeży checkout serwowałby kod, którego nikt w tym
 * biegu nie tknął — czyli ten sam błąd, tylko drożej. Dlatego ten jeden panel podaje wspólnej
 * kontrolce listę odpowiedzi; brzmienia i pytanie bierze takie, jak wszyscy
 * (`./where-it-works.tsx`).
 *
 * 2026-08-31 — WŁASNEJ LISTY TU JUŻ NIE MA, z tego samego powodu, co w `check-panel.tsx`. */
const OFFERS = ['project', 'same-copy'] as const;

/* `ROW` i `LABEL` zniknęły 2026-08-31 — patrz `checkpoint-panel.tsx`: rolę niosą teraz
 * `.stack` (etykieta nad kontrolką) i `.label` (etykieta pola), a zdanie pod kontrolką ma
 * własną, inną rolę (`.lead`). */
/* Klasa domu, nie własny opis — ten sam powód, co w `checkpoint-panel.tsx`. */
const FIELD = 'field';

export function ServePanel({
  step,
  onEditStep,
  stepBefore = null,
  handsItOver = false,
  onAskTheStepBefore = () => undefined,
}: ServePanelProps): ReactElement {
  return (
    <>
      <div className="stack">
        <label htmlFor="serve-name" className="label">
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

      <div className="stack">
        <label htmlFor="serve-command" className="label">
          Command to run
        </label>
        <input
          id="serve-command"
          className={FIELD}
          placeholder="npm run dev"
          value={step.command}
          disabled={step.commandFrom !== undefined}
          onChange={(event) => {
            onEditStep({ command: event.target.value });
          }}
        />
        {/* PRZEŁĄCZNIK, NIE DRUGIE POLE OBOK. Komenda ma jedno źródło naraz — wpisana ręcznie
            albo oddana przez krok przed tym — a dwa wypełnione pola obok siebie każą człowiekowi
            zgadywać, które wygra. Pole wyżej gaśnie, kiedy wygrywa krok przed tym, więc widać to
            bez czytania czegokolwiek.

            Nazwa pola jest ustalona i nie ma kontrolki: to jest ta sama nazwa, o którą krok przed
            tym jest proszony w swoim „What it hands over", a dwie nazwy do uzgodnienia w dwóch
            miejscach są pierwszą rzeczą, która się rozjedzie — i rozjazd widać dopiero jako bieg,
            który dochodzi do tego kafelka po to, żeby odmówić. */}
        <label className="flex items-baseline gap-2 text-body text-ink">
          <input
            type="checkbox"
            data-field="commandFrom"
            checked={step.commandFrom !== undefined}
            onChange={(event) => {
              onEditStep({
                /* NAZWA LICZONA RAZ, PRZY ZAZNACZENIU, i od tej chwili zapisana. Przeliczana
                   przy każdym renderze znaczyłaby, że przemianowanie kafelka po cichu rozłącza
                   graf — powód w całości stoi przy `fieldNameFor`. */
                commandFrom: event.target.checked ? { field: fieldNameFor(step) } : undefined,
              });
            }}
          />
          Let the step before this one work out the command
        </label>
        {step.commandFrom === undefined ? null : (
          /* TRZY STANY, NIE JEDEN NAPIS. „Poproś go" bez powiedzenia, czy już poproszono, każe
             człowiekowi sprawdzać drugi kafelek za każdym razem; a kafelek bez poprzednika
             odsyła go do kroku, którego nie ma. */
          <span className="lead" data-field="commandFromState">
            {stepBefore === null
              ? 'Nothing points at this tile yet, so there is nobody to work the command out. ' +
                'Draw an arrow from the step that should.'
              : handsItOver
                ? `${stepBefore} hands over “${step.commandFrom.field}”, and this tile runs it.`
                : `${stepBefore} does not hand over “${step.commandFrom.field}” yet, so this ` +
                  `tile would have nothing to run.`}
          </span>
        )}
        {step.commandFrom === undefined || handsItOver || stepBefore === null ? null : (
          /* JAWNY PRZYCISK, NIE EFEKT UBOCZNY ZAZNACZENIA. Kliknięcie w ten kafelek, które po
             cichu zmienia SĄSIEDNI, jest rodzajem magii, przez którą przestaje się ufać
             edytorowi — a tutaj widać, co się stanie, zanim się to zrobi. */
          <button
            type="button"
            data-field="askTheStepBefore"
            className="text-left text-label text-accent hover:underline"
            onClick={onAskTheStepBefore}
          >
            Ask {stepBefore} for it
          </button>
        )}
        {/* To zdanie jest CAŁĄ różnicą między tym kafelkiem a krokiem „sprawdź" i musi stać
            tam, gdzie człowiek podejmuje decyzję (niezmiennik 29). Druga połowa mówi, DOKĄD ta
            rzecz idzie i kiedy umiera: bez niej człowiek nie wie, czy zostanie mu w tle serwer
            trzymający port. „Started" jest nazwą TEJ SEKCJI z ekranu biegu (`rail.tsx`), nie
            naszym słowem — człowiek ma szukać tego, co widzi (niezmiennik 13). */}
        <span className="lead">
          The steps after this one start right away, without waiting for it to finish. It stays
          alive under Started on the right until you stop it there or close Loadout.
        </span>
      </div>

      <WhereItWorks
        group="serve-where"
        offers={OFFERS}
        value={step.folder}
        onChoose={(folder) => {
          onEditStep({ folder });
        }}
      />
    </>
  );
}
