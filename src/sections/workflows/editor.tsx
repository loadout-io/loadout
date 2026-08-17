/* Edytor workflow: płótno plus panel kroku — ekran, po który ta sekcja w ogóle istnieje.
 *
 * DLACZEGO TEN PLIK POWSTAŁ. `canvas.tsx` (płótno), `step-panel/panel.tsx`, `skills-row.tsx`
 * i `checkpoint-panel.tsx` istnieją, mają testy i do 2026-08-17 **nie miały ani jednego miejsca
 * montowania**: sekcja Workflow renderowała wyłącznie listę, więc do edytora nie prowadziło
 * ani jedno kliknięcie. To ta sama rodzina, co zaślepki adapterów i `Limiter` z T-21 —
 * mechanizm wylądował, ma testy, nikt go nie podłączył. Test renderujący komponent wprost
 * nie odróżnia „zamontowane" od „istnieje".
 *
 * Magazyn dokumentu powstaje NA OTWARTY PLIK i ginie przy zamknięciu: `createWorkflowStore`
 * bierze dokument w konstruktorze, bo magazyn bez dokumentu nie ma sensu (`state/workflows.ts`).
 * Trzymanie jednego magazynu na całą sekcję wymagałoby `document: null`, czyli stanu, który
 * tamten plik świadomie wyklucza.
 */
import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';

import type { Agent } from '../../state/agents';
import type { AgentStep, WorkflowFile } from '../../state/workflows';
import { createWorkflowStore } from '../../state/workflows';
import * as agentsIo from '../agents/io';
import { WorkflowCanvas } from './canvas/canvas';
import * as disk from './io';
import { StepPanel } from './step-panel/panel';

const QUIET = 'h-7 rounded-sq border border-line px-3 text-ui text-body';

export interface WorkflowEditorProps {
  /** Nazwa pliku, pod którą ten dokument leży na dysku. */
  path: string;
  /** Dokument wczytany przez sekcję — edytor go nie ładuje, tylko pokazuje i zmienia. */
  document: WorkflowFile;
  /** Agenci z biblioteki: panel kroku pokazuje wartości efektywne, więc musi znać agenta. */
  agents: readonly Agent[];
  onClose: () => void;
  onRun: (path: string) => void;
}

export function WorkflowEditor({
  path,
  document,
  agents,
  onClose,
  onRun,
}: WorkflowEditorProps): ReactElement {
  /* Magazyn powstaje DOKŁADNIE RAZ na zamontowanie tego ekranu — inicjalizator `useState`
   * biegnie tylko przy pierwszym renderze.
   *
   * Dokument jest tu ZIARNEM, nie wejściem: magazyn od tej chwili sam go trzyma i zmienia,
   * a przebudowa przy każdej edycji kasowałaby uwagi walidatora i odliczanie autosave'u.
   * Wymiana pliku odbywa się więc przez PRZEMONTOWANIE — sekcja podaje `key={path}` — a nie
   * przez tablicę zależności, której `react-hooks` musiałaby pilnować wbrew sobie. Pierwsza
   * wersja stała na `useMemo` z ręcznie przyciętą listą zależności i dwoma wyciszeniami reguły
   * hooków; odrzuciło je `quick-suppressions` i miało rację: wyciszenie ostrzeżenia jest tańsze
   * niż poprawny kształt tylko do chwili, w której ktoś dopisze tu drugie wejście. */
  const [store] = useState(() =>
    createWorkflowStore(
      {
        /* `write` bierze ścieżkę i dokument, `WorkflowIo.save` — sam dokument. Ścieżka jest
         * domknięta tutaj, bo to edytor wie, który plik ma otwarty; magazyn tego nie wie
         * i nie powinien (drugie miejsce z odpowiedzią „gdzie to leży"). */
        save: (file) => disk.write(path, file),
        check: disk.check,
        /* Zapis AGENTA, nie kroku. Panel ma w liście „Save to the agent", a ta droga jest
         * jedyną, przez którą wolno jej dojechać do pliku agenta (`state/workflows.ts` §8). */
        saveAgent: agentsIo.save,
      },
      document,
    ),
  );

  const state = store();
  const [openStepId, setOpenStepId] = useState<string | null>(null);

  /* Uwagi walidatora bierzemy przy otwarciu, nie dopiero po pierwszej zmianie: workflow zapisany
   * wczoraj i zepsuty od wczoraj ma powiedzieć o tym od razu, a nie po dotknięciu kafelka.
   *
   * Czytamy przez `store.getState()`, a nie przez migawkę `state`: migawka zmienia się przy
   * każdej edycji, więc w zależnościach kazałaby temu efektowi biec po każdym naciśnięciu
   * klawisza. Magazyn jest stały przez całe życie tego ekranu, więc lista zależności jest
   * uczciwa i kompletna — bez wyciszania czegokolwiek. */
  useEffect(() => {
    void store.getState().recheck();
  }, [store]);

  const open = state.document.steps.find(
    (step): step is AgentStep => step.kind === 'agent' && step.id === openStepId,
  );
  const agentOf = open === undefined ? undefined : agents.find((a) => a.id === open.agent);

  return (
    <section className="flex h-full min-h-0 flex-col">
      <header className="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-4">
        <button type="button" className={QUIET} onClick={onClose}>
          ←
        </button>
        <h1 className="text-title text-ink">{state.document.name}</h1>
        <span className="font-mono text-mono text-muted">{path}</span>
        {/* Uwagi walidatora są faktem o dokumencie i mieszkają w jednym miejscu — tutaj.
         * Płótno ich nie liczy i nie tłumaczy (`canvas.tsx`). */}
        {state.notes.length === 0 ? null : (
          <span className="rounded-sq border border-attend-edge bg-attend-wash px-2 font-mono text-label text-attend">
            {state.notes.length === 1 ? '1 problem' : `${String(state.notes.length)} problems`}
          </span>
        )}
      </header>

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_330px]">
        <div className="min-h-0 overflow-auto p-4">
          <WorkflowCanvas
            document={state.document}
            notes={state.notes}
            onChange={state.commit}
            onRun={() => {
              onRun(path);
            }}
            onOpenPanel={setOpenStepId}
          />
        </div>

        {/* Panel kroku, szerokość 330 px prosto z makiety (`.side`). Bez otwartego kroku
         * kolumna zostaje pusta, zamiast znikać: znikająca kolumna przesuwa płótno pod
         * kursorem w chwili kliknięcia. */}
        <aside className="min-h-0 overflow-auto border-l border-line bg-panel p-4">
          {open === undefined || agentOf === undefined ? (
            <p className="text-muted">Pick a step to see what it was given.</p>
          ) : (
            <StepPanel
              step={open}
              agent={agentOf}
              onEdit={(edit) => {
                /* Agent jedzie ARGUMENTEM, bo panel podaje wartości EFEKTYWNE, a różnicę
                 * wobec agenta liczy magazyn (`applyPanelEdit`). Bez tego edytor musiałby
                 * wiedzieć, co jest nadpisaniem, a co dziedziczeniem — czyli drugi raz. */
                state.editStep(open.id, agentOf, edit);
              }}
              onEditStep={(fields) => {
                /* Pola własne kroku (nazwa, instrukcje, `copies`) nie są nadpisaniami agenta,
                 * więc nie mają osobnej akcji: jadą przez `commit`, czyli tę jedną drogę,
                 * którą nowy dokument wchodzi do stanu (i pod którą wisi autosave). */
                state.commit({
                  ...state.document,
                  steps: state.document.steps.map((step) =>
                    step.id === open.id ? { ...step, ...fields } : step,
                  ),
                });
              }}
              onReset={(field) => {
                state.resetRow(open.id, field);
              }}
            />
          )}
        </aside>
      </div>
    </section>
  );
}
