/* Panel kroku po prawej — siedem wierszy z makiety (`docs/mockup/index.html:599-618`).
 *
 * SZKIELET — ciało rzuca `not implemented` (AGENTS.md §2a, odpowiednik `todo!()`).
 *
 * Siedem etykiet, w tej kolejności, i ani jednej ósmej:
 *   Name · Who does this · What to do · How many at once · Can it change files ·
 *   Give up after · Write results to
 *
 * Trzy pierwsze należą do KROKU. Trzy ostatnie to wartości efektywne pochodzące z agenta, więc
 * niosą kropkę, szare `Agent uses: …` i `Reset`, kiedy krok się od agenta różni [T4 §4.5].
 * `Who does this` jest wierszem, który nazywa agenta, i to na jego etykiecie siedzi znacznik
 * „N changed" (makieta, linia 602) — razem z szarym wierszem dla każdego zmienionego ustawienia,
 * które nie ma własnej kontrolki.
 *
 * Liczba w znaczniku jest liczona z `step.overrides` TUTAJ i nigdzie indziej (niezmiennik 13:
 * jeden fakt, jedno miejsce). Osobny licznik trzymany w stanie kroku rozjeżdża się z patchem
 * przy pierwszym `Reset` i nikt tego nie zauważy, bo obie liczby wyglądają wiarygodnie.
 *
 * Czego tu NIE MA: przełącznika „Let it split into helpers" z makiety (linia 625). Żadne pole
 * schematu go nie niesie, a T3 §7.3 i T4 §3.3 zgodnie wykluczają głębokość delegacji z v1.
 * Przepisanie makiety jeden do jednego jest tu dokładnie tym, jak łamie się niezmiennik 16:
 * trzeci przełącznik wygląda identycznie jak dwa działające.
 *
 * Czego tu nie ma z innego powodu: wiersza Skills. Jest osobnym komponentem (`skills-row.tsx`),
 * bo znika w całości przy agencie na Codeksie i ma własny tryb. Składa je ekran, jedno piętro
 * wyżej — dzięki temu „siedem etykiet" jest równością, a nie „siedem plus to, co akurat dołożył
 * wiersz umiejętności".
 *
 * Panel jest STEROWANY — wartości i każde kliknięcie wychodzą propsami. Powód jest testowy:
 * w repo nie ma `jsdom` ani `@testing-library/react` (`package.json` jest na liście DENIED
 * w `checks/quick-scope.sh`), więc panel sprawdzamy przez `renderToStaticMarkup`, a stan
 * trzymany wewnątrz komponentu byłby dla takiego testu niewidoczny.
 */
import type { ReactElement } from 'react';
import type { Agent, FileAccess } from '../../../state/agents';
import type { AgentStep, OverridableField, Overrides } from '../../../state/workflows';
import { resolve } from './overrides';

export interface StepPanelProps {
  step: AgentStep;
  /** Agent wskazany przez krok. Panel czyta go, żeby pokazać wartości efektywne — i NIGDY go
   * nie zapisuje (`docs/mockup/index.html:604`). */
  agent: Agent;
  /** Zmiana wiersza pochodzącego z agenta, podana wartością efektywną. */
  onEdit: (edit: Overrides) => void;
  /** Zmiana pola, które należy do samego kroku. */
  onEditStep: (fields: Partial<Pick<AgentStep, 'name' | 'instructions' | 'copies'>>) => void;
  /** `Reset` przy jednym wierszu. */
  onReset: (field: OverridableField) => void;
}

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const FIELD = 'h-8 rounded-sq border border-line bg-well px-2 text-body text-ink';
const AREA = 'min-h-24 rounded-sq border border-line bg-well p-2 text-body text-ink';
/* `chip`, wariant neutralny (DESIGN §6): licznik zmian nie jest stanem biegu, więc nie bierze
 * żadnego z czterech kolorów stanu. */
const CHIP = 'rounded-sq border border-line bg-raised px-2 text-label text-muted';
const QUIET = 'text-label text-muted underline';
const FROM_AGENT = 'text-label text-muted';

/* Brzmienia wartości — te same, które ma formularz agenta (`src/sections/agents/agent-form.tsx`).
 * Druga kopia, świadoma: tamten plik ich nie eksportuje i nie należy do tego zadania, a wpisanie
 * `look-only` w szary wiersz „Agent uses:" wpuściłoby nazwę z drutu na ekran (niezmiennik 14).
 * Wspólny moduł brzmień jest właściwym domem dla obu — kiedy ktoś będzie posiadał oba pliki. */
const FILE_ACCESS: ReadonlyArray<{ value: FileAccess; label: string }> = [
  { value: 'look-only', label: 'Look only' },
  { value: 'ask-first', label: 'Ask first' },
  { value: 'work-freely', label: 'Work freely' },
];

const THINKING: Record<Agent['thinking'], string> = {
  quick: 'Quick',
  balanced: 'Balanced',
  deep: 'Deep',
  deepest: 'Deepest',
};

/** Ile minut, po ludzku. `0` znaczy „bez limitu" i nigdy nie jest pustym polem [T4 §4.3]. */
function minutes(value: number): string {
  return value === 0 ? 'no limit' : `${String(value)} minutes`;
}

/** Wartość, którą wziąłby agent, jako zdanie po angielsku.
 *
 * To jest treść szarego wiersza `Agent uses: …`, więc nie ma prawa być nazwą z drutu ani
 * `[object Object]`: użytkownik czyta ją zamiast otwierać sekcję Agents [T4 §4.5]. */
function agentUses(field: OverridableField, agent: Agent): string {
  switch (field) {
    case 'thinking':
      return THINKING[agent.thinking];
    case 'fileAccess':
      return FILE_ACCESS.find((one) => one.value === agent.fileAccess)?.label ?? '';
    case 'giveUpAfterMinutes':
      return minutes(agent.giveUpAfterMinutes);
    case 'tools':
      return agent.tools === 'everything' ? 'Everything' : agent.tools.only.join(', ');
    case 'skills':
      return agent.skills.length === 0 ? 'none' : agent.skills.join(', ');
    case 'connections':
      return agent.connections.length === 0 ? 'none' : agent.connections.join(', ');
    case 'instructions':
      return agent.instructions;
    case 'model':
      return agent.model;
    case 'writeResultsTo':
      return agent.writeResultsTo;
  }
}

/** Etykiety wierszy, które mają własną kontrolkę. Reszta zmienionych ustawień pokazuje się
 * szarym wierszem pod „Who does this" — patrz `noRowOfTheirOwn`. */
const HAS_A_ROW: readonly OverridableField[] = [
  'fileAccess',
  'giveUpAfterMinutes',
  'writeResultsTo',
];

/** Zmienione ustawienia, których nie widać w żadnym z siedmiu wierszy.
 *
 * Bez tego zmiana `thinking` znikałaby z ekranu: licznik mówiłby „1 changed", a użytkownik nie
 * miałby jak zobaczyć CZEGO ani jak to cofnąć. */
function noRowOfTheirOwn(changed: OverridableField[]): OverridableField[] {
  return changed.filter((field) => !HAS_A_ROW.includes(field));
}

export function StepPanel({
  step,
  agent,
  onEdit,
  onEditStep,
  onReset,
}: StepPanelProps): ReactElement {
  /* Wartości EFEKTYWNE do pokazania i lista zmienionych pól — jedno wywołanie, jeden fakt.
   * Licznik „N changed" jest długością tej listy i nie istnieje nigdzie indziej: osobna liczba
   * trzymana w stanie kroku rozjeżdża się z patchem przy pierwszym `Reset`, a obie wyglądają
   * wiarygodnie (niezmiennik 13). */
  const { agent: effective, changed } = resolve(agent, step.overrides);

  /** Kropka, `Reset` i szary wiersz — wszystko, co odróżnia wiersz zmieniony od dziedziczonego. */
  const mark = (field: OverridableField) =>
    changed.includes(field) ? (
      <>
        <span className={FROM_AGENT}>●</span>
        <button type="button" className={QUIET} onClick={() => onReset(field)}>
          Reset
        </button>
      </>
    ) : null;

  const wasUsing = (field: OverridableField) =>
    changed.includes(field) ? (
      <span className={FROM_AGENT}>Agent uses: {agentUses(field, agent)}</span>
    ) : null;

  return (
    <aside className="flex w-82 flex-col gap-3 border-l border-line bg-panel p-4" data-step-panel>
      <div className={ROW}>
        <label htmlFor="step-name" className={LABEL}>
          Name
        </label>
        <input
          id="step-name"
          className={FIELD}
          value={step.name}
          onChange={(event) => {
            onEditStep({ name: event.target.value });
          }}
        />
      </div>

      <div className={ROW}>
        <div className="flex items-baseline gap-2">
          <label className={LABEL}>Who does this</label>
          {changed.length > 0 ? <span className={CHIP}>{changed.length} changed</span> : null}
        </div>
        {/* Nazwa agenta, nie lista wyboru: żeby wybrać innego agenta, ten komponent musiałby
            dostać listę agentów i handler wyboru, a kontrolka bez handlera nie wchodzi do repo
            (niezmiennik 16). */}
        <span className="text-body text-ink">
          {agent.name} — {agent.summary}
        </span>
        <span className={FROM_AGENT}>
          This comes from the agent. Changing it here does not change the agent.
        </span>
        {noRowOfTheirOwn(changed).map((field) => (
          <div key={field} className="flex items-baseline gap-2">
            {mark(field)}
            {wasUsing(field)}
          </div>
        ))}
      </div>

      <div className={ROW}>
        <label htmlFor="step-instructions" className={LABEL}>
          What to do
        </label>
        <textarea
          id="step-instructions"
          className={AREA}
          value={step.instructions}
          onChange={(event) => {
            onEditStep({ instructions: event.target.value });
          }}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="step-copies" className={LABEL}>
          How many at once
        </label>
        <input
          id="step-copies"
          className={FIELD}
          type="number"
          min={1}
          max={8}
          value={String(step.copies)}
          onChange={(event) => {
            onEditStep({ copies: copiesFrom(event.target.value) });
          }}
        />
        <span className={FROM_AGENT}>
          More than one only helps when the copies work on different questions.
        </span>
      </div>

      <div className={ROW}>
        <div className="flex items-baseline gap-2">
          <label htmlFor="step-file-access" className={LABEL}>
            Can it change files
          </label>
          {mark('fileAccess')}
        </div>
        <select
          id="step-file-access"
          className={FIELD}
          value={effective.fileAccess}
          onChange={(event) => {
            onEdit({ fileAccess: fileAccessFrom(event.target.value, effective.fileAccess) });
          }}
        >
          {FILE_ACCESS.map((one) => (
            <option key={one.value} value={one.value}>
              {one.label}
            </option>
          ))}
        </select>
        {wasUsing('fileAccess')}
      </div>

      <div className={ROW}>
        <div className="flex items-baseline gap-2">
          <label htmlFor="step-give-up-after" className={LABEL}>
            Give up after
          </label>
          {mark('giveUpAfterMinutes')}
        </div>
        <input
          id="step-give-up-after"
          className={FIELD}
          type="number"
          min={0}
          value={String(effective.giveUpAfterMinutes)}
          onChange={(event) => {
            onEdit({ giveUpAfterMinutes: minutesFrom(event.target.value) });
          }}
        />
        {wasUsing('giveUpAfterMinutes')}
      </div>

      <div className={ROW}>
        <div className="flex items-baseline gap-2">
          <label htmlFor="step-write-results-to" className={LABEL}>
            Write results to
          </label>
          {mark('writeResultsTo')}
        </div>
        <input
          id="step-write-results-to"
          className={FIELD}
          value={effective.writeResultsTo}
          onChange={(event) => {
            onEdit({ writeResultsTo: event.target.value });
          }}
        />
        {wasUsing('writeResultsTo')}
      </div>
    </aside>
  );
}

/** 1–8 kopii [T3 §4.4]. Poza zakresem wraca do jedynki, bo „zero kopii" to krok, który nie
 * biegnie, a taki krok kasuje się, nie zeruje. */
function copiesFrom(raw: string): number {
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed)) return 1;
  return Math.min(8, Math.max(1, parsed));
}

/** „Bez limitu" to zero, nigdy pusta wartość [T4 §4.3, reguła 1]. */
function minutesFrom(raw: string): number {
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

/** Wariant z listy albo dotychczasowy. Rzutowanie napisu z DOM-u na wariant enuma byłoby
 * obietnicą, której ten napis nie składa. */
function fileAccessFrom(raw: string, now: FileAccess): FileAccess {
  return FILE_ACCESS.find((one) => one.value === raw)?.value ?? now;
}
