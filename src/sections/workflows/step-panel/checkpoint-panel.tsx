/* Panel punktu kontrolnego — dwa wiersze, bo punkt kontrolny ma dwa pola.
 *
 * Istnieje z jednego powodu i ten powód jest niezmiennikiem 16: płótno ma przycisk
 * `＋ Add a checkpoint` (makieta, linia 529), a przycisk, który tworzy kafelek bez sposobu na
 * nazwanie go i zadanie pytania, jest kontrolką prowadzącą donikąd. Kafelek z pustym pytaniem
 * wygląda na płótnie dokładnie tak samo jak wypełniony.
 *
 * To NIE jest `StepPanel` z siódemką wierszy: punkt kontrolny nie ma agenta, więc nie ma
 * dziedziczenia, nie ma nadpisań, nie ma licznika „N changed" i nie ma wiersza Skills. Wspólny
 * komponent dla obu rodzajów kroku byłby formularzem, w którym połowa wierszy jest schowana
 * warunkiem — a to jest ta sama konstrukcja, którą DESIGN §6 nazywa zakładkami w panelu.
 *
 * Sterowany, tak jak `StepPanel` i z tego samego powodu (nagłówek `panel.tsx`).
 */
import type { ReactElement } from 'react';
import type { CheckpointStep } from '../../../state/workflows';

export interface CheckpointPanelProps {
  step: CheckpointStep;
  onEditStep: (fields: Partial<Pick<CheckpointStep, 'name' | 'question'>>) => void;
}

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const FIELD = 'h-8 rounded-sq border border-line bg-well px-2 text-body text-ink';

export function CheckpointPanel({ step, onEditStep }: CheckpointPanelProps): ReactElement {
  return (
    <aside className="flex w-82 flex-col gap-3 border-l border-line bg-panel p-4" data-step-panel>
      <div className={ROW}>
        <label htmlFor="checkpoint-name" className={LABEL}>
          Name
        </label>
        <input
          id="checkpoint-name"
          className={FIELD}
          value={step.name}
          onChange={(event) => {
            onEditStep({ name: event.target.value });
          }}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="checkpoint-question" className={LABEL}>
          What to ask
        </label>
        <input
          id="checkpoint-question"
          className={FIELD}
          /* `?? ''` trzyma pole STEROWANYM. `value={undefined}` oddaje je Reactowi jako pole
           * niesterowane i od tej chwili napisany tekst żyje wyłącznie w DOM-ie — czyli nie
           * dojeżdża do pliku, a pole i tak wygląda na wypełnione. */
          value={step.question ?? ''}
          onChange={(event) => {
            onEditStep({ question: event.target.value });
          }}
        />
        <span className={LABEL}>The run stops here until you answer.</span>
      </div>
    </aside>
  );
}
