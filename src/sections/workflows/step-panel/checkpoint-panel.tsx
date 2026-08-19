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
 *
 * 2026-08-18 — RAMKI TU JUŻ NIE MA. Ten plik rysował `<aside class="w-82 border-l bg-panel p-4">`
 * wewnątrz `<aside>` szerokości 330 px z `editor.tsx`, czyli podwójny obrys, podwójny padding
 * i poziomy pasek przewijania. Ramkę ma teraz `PanelForStep`, jedną, dla wszystkich trzech
 * paneli — razem ze znacznikiem `data-step-panel`, który był przy okazji trzecim miejscem,
 * w którym trzeba było pamiętać o jego dopisaniu.
 */
import type { ReactElement } from 'react';
import type { CheckpointStep } from '../../../state/workflows';

export interface CheckpointPanelProps {
  step: CheckpointStep;
  onEditStep: (fields: Partial<Pick<CheckpointStep, 'name' | 'question'>>) => void;
}

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
/* POLE BIERZE KLASE DOMU, NIE WLASNY OPIS.
 *
 * `theme.css` ma klase `.field` od pierwszego dnia: studnia, mocny obrys, promien z pasma, kroj
 * maszynowy i `user-select: text` — to ostatnie jest czescia pola, nie ozdoba, bo `body` wylacza
 * zaznaczanie w calej aplikacji. Do 2026-08-19 wolaly ja DWA miejsca, a cztery sekcje przepisywaly
 * ten sam wyglad recznie w dwunastu stalych — i rozjechaly sie: tu obrys byl `--line`, w Skills
 * `--line-strong`. Jeden fakt, jedno miejsce (niezmiennik 13); dwa opisy tego samego pola czyta
 * sie jak dwa rozne stany, a nie jak dwa pola.
 *
 * Skupienia tu nie ma z tego samego powodu. `theme.css` daje `.field:focus` obwodke w akcencie
 * i globalny `:focus-visible` obrys — jedna regula na cala aplikacje. Dopisanie tego samego
 * narzedziem na kazdym polu byloby trzecia kopia decyzji, ktora juz jest podjeta. */
const FIELD = 'field';

export function CheckpointPanel({ step, onEditStep }: CheckpointPanelProps): ReactElement {
  return (
    <>
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
    </>
  );
}
