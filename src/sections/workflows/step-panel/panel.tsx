/* Panel kroku po prawej — siedem wierszy z makiety (`docs/mockup/index.html:599-618`).
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
 * bo znika w całości przy agencie na Codeksie i ma własny tryb. Składa je `PanelForStep` na
 * dole tego pliku — dzięki temu „siedem etykiet" jest równością, a nie „siedem plus to, co
 * akurat dołożył wiersz umiejętności".
 *
 * Panel jest STEROWANY — wartości i każde kliknięcie wychodzą propsami. Powód jest testowy:
 * w repo nie ma `jsdom` ani `@testing-library/react` (`package.json` jest na liście DENIED
 * w `checks/quick-scope.sh`), więc panel sprawdzamy przez `renderToStaticMarkup`, a stan
 * trzymany wewnątrz komponentu byłby dla takiego testu niewidoczny.
 *
 * 2026-08-18, WIECZÓR — DWIE NAPRAWY, KTÓRE SĄ POWODEM PIERWSZEGO ZDANIA WŁAŚCICIELA
 * („ustawiasz workflow ale agentów nie da się wybrać"):
 *
 *   1. Lista wyboru agenta stała WYŁĄCZNIE w `PickAnAgent`, czyli w panelu kroku, którego
 *      agenta NIE DA SIĘ rozwiązać. Po pierwszym wyborze panel przełączał się na `StepPanel`,
 *      a tam wiersz „Who does this" był nieklikalnym `<span>` z nazwą agenta — komentarz obok
 *      przyznawał to wprost. Pomyłka przy wyborze była więc NIEODWRACALNA z okna: jedyną drogą
 *      naprawy było otwarcie pliku JSON w edytorze tekstu. Wybór jest teraz w JEDNYM miejscu
 *      (`AgentChoice`) i oba panele montują to samo.
 *   2. Panel świeżego kroku miał dwa wiersze: Name i Who does this. Pola `What to do` nie było
 *      w nim wcale, a `<textarea id="step-instructions">` żyło tylko w `StepPanel`, czyli za
 *      blokadą z punktu 1. Dowód, że właściciel tam nie dotarł: oba jego pliki workflow mają
 *      `"instructions": ""`. Instrukcje kroku są teraz edytowalne od pierwszej chwili — razem
 *      z `How many at once`, bo to też jest pole KROKU i nie potrzebuje agenta.
 */
import type { ReactElement, ReactNode } from 'react';
import type { Agent, FileAccess } from '../../../state/agents';
import type {
  AgentStep,
  CheckpointStep,
  Folder,
  OverridableField,
  Overrides,
  SkillChoice,
  Step,
} from '../../../state/workflows';
import { SKILL_SUBSETTING } from './capabilities';
import { CheckpointPanel } from './checkpoint-panel';
import { resolve } from './overrides';
import { SkillsRow } from './skills-row';

/** Pola, które należą do samego KROKU agenta, a nie do agenta (patrz nagłówek pliku). */
export type AgentStepFields = Partial<
  Pick<AgentStep, 'name' | 'instructions' | 'copies' | 'folder'>
>;

/** Oba pola punktu kontrolnego. Punkt kontrolny nie dziedziczy niczego, więc to jest całość. */
export type CheckpointFields = Partial<Pick<CheckpointStep, 'name' | 'question'>>;

/** Pozycja `＋ Create a new agent…` z makiety (linia 603).
 *
 * Wartość jest napisem, którego żaden agent nie może nosić: identyfikatory są uuid v7
 * (`src/state/agents.ts`, `newId`), więc kolizja nie jest „mało prawdopodobna", tylko niemożliwa.
 * Bez wartości-wartownika trzeba by rozpoznawać tę pozycję po jej TEKŚCIE, a wtedy zmiana copy
 * cicho zamienia skrót w wybór agenta o nazwie, której nie ma. */
const CREATE_AN_AGENT = 'create-a-new-agent';

export interface StepPanelProps {
  step: AgentStep;
  /** Agent wskazany przez krok. Panel czyta go, żeby pokazać wartości efektywne — i NIGDY go
   * nie zapisuje (`docs/mockup/index.html:604`). */
  agent: Agent;
  /** Cała biblioteka: wiersz „Who does this" jest listą WYBORU, także po wyborze. */
  agents: readonly Agent[];
  /** Zmiana agenta na tym kroku. Nie jest nadpisaniem — to pole samego kroku. */
  onChooseAgent: (agentId: string) => void;
  /** Skrót do sekcji Agents z pozycji `＋ Create a new agent…`. */
  onCreateAgent: () => void;
  /** Zmiana wiersza pochodzącego z agenta, podana wartością efektywną. */
  onEdit: (edit: Overrides) => void;
  /** Zmiana pola, które należy do samego kroku. */
  onEditStep: (fields: AgentStepFields) => void;
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

interface AgentChoiceProps {
  /** `step.agent`. Pusty napis znaczy „jeszcze nikt" i tak wychodzi krok z `＋ Add step`. */
  chosen: string;
  agents: readonly Agent[];
  onChooseAgent: (agentId: string) => void;
  onCreateAgent: () => void;
}

/** Kontrolka wyboru agenta — JEDNA na całą sekcję i to jest cały sens tego komponentu.
 *
 * Do 2026-08-18 istniała tylko w panelu kroku BEZ agenta, więc zmiana raz podjętej decyzji była
 * niemożliwa z okna. Dwie kopie tej listy — jedna „przed wyborem", druga „po" — byłyby dwoma
 * miejscami, w których mieszka odpowiedź na pytanie „jak wybiera się agenta" (niezmiennik 13),
 * i pierwszą okazją, żeby jedna z nich zapisywała coś innego niż druga.
 *
 * Pusta biblioteka nie dostaje listy, tylko zdanie i DROGĘ WYJŚCIA. Wcześniej stało tam samo
 * zdanie („Make one in Agents, then come back") i ani jednej ścieżki z powrotem — czyli
 * instrukcja nawigacji zamiast nawigacji. */
function AgentChoice({
  chosen,
  agents,
  onChooseAgent,
  onCreateAgent,
}: AgentChoiceProps): ReactElement {
  if (agents.length === 0) {
    return (
      <>
        <span className={FROM_AGENT}>You have not saved anyone yet.</span>
        <button type="button" className={QUIET} onClick={onCreateAgent}>
          ＋ Create a new agent
        </button>
      </>
    );
  }

  /* „Pick one" stoi w liście DOKŁADNIE wtedy, gdy nikt jeszcze nie jest wybrany. Pozycja pusta
   * zostawiona na stałe pozwala wybrać ją z powrotem, a `agent: ''` jako decyzja to krok, który
   * przy Run odmawia zdaniem o brakującym agencie — czyli cofnięcie się do stanu wyjściowego
   * bez powiedzenia o tym ani słowa. */
  const nobodyYet = !agents.some((one) => one.id === chosen);

  return (
    <select
      id="step-agent"
      className={FIELD}
      value={nobodyYet ? '' : chosen}
      onChange={(event) => {
        const picked = event.target.value;
        /* Skrót do sekcji Agents rozpoznajemy po WARTOŚCI, nie po tekście pozycji. */
        if (picked === CREATE_AN_AGENT) {
          onCreateAgent();
          return;
        }
        /* Pozycja-zaproszenie nie jest wyborem. Bez tego warunku otwarcie listy i zamknięcie
         * jej bez decyzji zapisywałoby `agent: ''` jako decyzję. */
        if (picked !== '') onChooseAgent(picked);
      }}
    >
      {nobodyYet ? <option value="">Pick one</option> : null}
      {agents.map((one) => (
        <option key={one.id} value={one.id}>
          {one.name} — {one.summary}
        </option>
      ))}
      <option value={CREATE_AN_AGENT}>＋ Create a new agent…</option>
    </select>
  );
}

interface WhoDoesThisProps extends AgentChoiceProps {
  /** Znacznik „N changed" — tylko panel z rozwiązanym agentem ma co policzyć. */
  chip?: ReactNode;
  /** Zdanie pod listą. Różne w obu panelach, bo mówią o różnych rzeczach. */
  note: string;
  /** Szare wiersze „Agent uses: …" dla ustawień bez własnej kontrolki. */
  inherited?: ReactNode;
}

/** Wiersz „Who does this" — jedna etykieta, jedna lista wyboru, w obu panelach ta sama. */
function WhoDoesThis({ chip, note, inherited, ...choice }: WhoDoesThisProps): ReactElement {
  return (
    <div className={ROW}>
      <div className="flex items-baseline gap-2">
        {/* `htmlFor` celuje w `select` z `AgentChoice`. Przy pustej bibliotece tego pola nie ma
            i etykieta zostaje bez celu — świadomie: alternatywą jest pusta lista wyboru, czyli
            kontrolka, która nie ma czego zrobić (niezmiennik 16). */}
        <label htmlFor="step-agent" className={LABEL}>
          Who does this
        </label>
        {chip}
      </div>
      <AgentChoice {...choice} />
      <span className={FROM_AGENT}>{note}</span>
      {inherited}
    </div>
  );
}

/** Wiersz `Name` — pole samego kroku, więc identyczny w obu panelach. */
function NameRow({
  value,
  onEditStep,
}: {
  value: string;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div className={ROW}>
      <label htmlFor="step-name" className={LABEL}>
        Name
      </label>
      <input
        id="step-name"
        className={FIELD}
        value={value}
        onChange={(event) => {
          onEditStep({ name: event.target.value });
        }}
      />
    </div>
  );
}

/** Wiersz `What to do` — prompt KROKU, nie nadpisanie agenta.
 *
 * Osobny komponent, bo od 2026-08-18 stoi w OBU panelach. Wcześniej stał tylko w tym z agentem,
 * czyli za blokadą wyboru: pole, w które trafia jedyne zdanie mówiące, co ten krok ma zrobić,
 * było nieosiągalne dla każdego świeżo dodanego kroku. */
function WhatToDoRow({
  value,
  onEditStep,
}: {
  value: string;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
    <div className={ROW}>
      <label htmlFor="step-instructions" className={LABEL}>
        What to do
      </label>
      <textarea
        id="step-instructions"
        className={AREA}
        value={value}
        onChange={(event) => {
          onEditStep({ instructions: event.target.value });
        }}
      />
    </div>
  );
}

/** Wiersz `How many at once` — też pole samego kroku, więc też w obu panelach. */
function CopiesRow({
  value,
  onEditStep,
}: {
  value: number;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  return (
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
        value={String(value)}
        onChange={(event) => {
          onEditStep({ copies: copiesFrom(event.target.value) });
        }}
      />
      <span className={FROM_AGENT}>
        More than one only helps when the copies work on different questions.
      </span>
    </div>
  );
}

/** Wiersz „Try again up to" — liczba rund POWROTU wychodzącego z tego kroku.
 *
 * NIE MA GO W MAKIECIE, i mówię to wprost, bo makieta jest jedyną wyrocznią wyglądu tej
 * aplikacji. Grep na `loop`, `again`, `retry` i `turns` w `docs/mockup/index.html` nie daje ani
 * jednego trafienia: pętla powstała po makiecie, na prośbę właściciela, więc tej kontrolki nie ma
 * skąd przepisać. Kształt jest zapożyczony z wiersza „How many at once", bo pyta o to samo —
 * o jedną liczbę należącą do kroku.
 *
 * DLACZEGO NA KROKU, SKORO LICZBA NALEŻY DO STRZAŁKI. Panelu strzałki w tym repo nie ma i nie
 * powstaje przy okazji. Rust już rozstrzygnął, gdzie to postawić: uwaga o złym zakresie
 * (`check::turns_out_of_range`) czepia się kroku, z którego powrót WYCHODZI — „to on jest sędzią
 * pętli i to jego kafelek człowiek otworzy, żeby zmienić tę liczbę" — a kliknięcie tej uwagi
 * otwiera panel dokładnie tego kroku. Postawienie kontrolki gdzie indziej znaczyłoby, że uwaga
 * prowadzi w miejsce, w którym nie da się jej spełnić.
 *
 * WIERSZA NIE MA, GDY Z KROKU NIE WYCHODZI POWRÓT. Pole „ile rund" przy kroku bez pętli jest
 * kontrolką bez skutku (niezmiennik 16) — i to gorszego rodzaju, bo wyglądałoby na ustawienie,
 * które czeka na włączenie gdzie indziej. */
function TriesRow({
  value,
  onEditWayBack,
}: {
  value: number;
  onEditWayBack: (turns: number) => void;
}): ReactElement {
  return (
    <div className={ROW}>
      <label htmlFor="step-tries" className={LABEL}>
        Try again up to
      </label>
      <input
        id="step-tries"
        className={FIELD}
        type="number"
        min={1}
        max={10}
        value={String(value)}
        onChange={(event) => {
          onEditWayBack(turnsFrom(event.target.value));
        }}
      />
      <span className={FROM_AGENT}>
        The tester sends the work back until it passes, or until the tries run out.
      </span>
    </div>
  );
}

/** Liczba tur z pola tekstowego, przycięta do zakresu, który przyjmuje plik.
 *
 * Pole `number` przepuszcza pustkę i tekst, a `Number('')` to zero — czyli pętla, która nie
 * wykonuje się ani razu i którą walidator odrzuca. Zaciskamy TUTAJ, bo dokument ma być poprawny
 * po każdym naciśnięciu klawisza: zapis leci autosavem 400 ms po ostatniej zmianie i nie ma
 * chwili, w której wolno mu być nieprawidłowy. To ta sama decyzja, co przy `copiesFrom`. */
function turnsFrom(typed: string): number {
  const value = Number.parseInt(typed, 10);
  if (Number.isNaN(value)) return 1;
  return Math.min(10, Math.max(1, value));
}

/** Przełącznik „Fresh copy of the files" — makieta, linia 620-621.
 *
 * 2026-08-19 — PO CO POWSTAŁ. Reguła `one_folder_two_steps` mówi krokom, które mogą biec
 * równocześnie w jednym folderze: „Give one of them a fresh copy". Do dziś aplikacja nie miała
 * ANI JEDNEGO miejsca, w którym dałoby się to zrobić: pole `folder` jest w schemacie od T-12,
 * jest w makiecie od początku, i nie miało kontrolki nigdzie w `src/`. Walidator kazał więc
 * zrobić rzecz, której to okno nie umiało — a jedynym wyjściem było ręczne poprawienie pliku
 * w edytorze tekstu. Zmierzone na workflow właściciela „Reaserch + implement": dwa kroki
 * researchu wchodzące do jednego kroku syntezy, czyli najzwyklejszy wachlarz, i ślepy zaułek.
 *
 * PRZEŁĄCZNIK, NIE ÓSMY WIERSZ Z ETYKIETĄ. Makieta stawia to w rzędzie przełączników pod
 * siedmioma polami — razem z „Let it use skills" — i tam też stoi tutaj. Kryterium
 * `overrides.test.tsx` („dokładnie siedem wierszy, żadnego ósmego") mierzy pola z etykietą
 * i broni się przed dosypywaniem ustawień, których makieta nie zna; ten przełącznik jest w niej
 * od początku i niesie pole schematu, więc kryterium zostaje nietknięte, a nie objechane.
 *
 * DWIE WARTOŚCI, NIE TRZY. `Folder::Pick { path }` istnieje w schemacie, ale nie ma go
 * w makiecie i nie da się go dziś nigdzie wskazać. Przełącznik przestawia WYŁĄCZNIE między
 * `project` a `fresh-copy`; krok z ręcznie wpisaną ścieżką pokazuje się jako wyłączony
 * i włączenie go jest świadomym „chcę własną kopię". Trzecia wartość udawana dwustanową
 * kontrolką kasowałaby cudzą ścieżkę bez pytania. */
/** Folder po jednym kliknięciu przełącznika.
 *
 * Osobna funkcja, bo to jest jedyna DECYZJA tego wiersza, a w tym repo nie ma jsdom — komponent
 * da się sprawdzić tylko przez statyczny render, który klika w nic. Rozstrzygnięcie o trzeciej
 * wartości (`pick`) zostałoby wtedy bez kryterium i skasowałby je pierwszy refaktor.
 *
 * `pick` wychodzi na `fresh-copy`, a nie na `project`: przełącznik pokazuje się dla niego jako
 * WYŁĄCZONY, więc jedyne kliknięcie, jakie ma sens, znaczy „chcę własną kopię". Wyjście
 * na `project` kasowałoby ręcznie wpisaną ścieżkę i robiłoby to pod pozorem włączania czegoś. */
export function nextFolder(value: Folder): Folder {
  return value.use === 'fresh-copy' ? { use: 'project' } : { use: 'fresh-copy' };
}

function FreshCopyRow({
  value,
  onEditStep,
}: {
  value: Folder;
  onEditStep: (fields: AgentStepFields) => void;
}): ReactElement {
  const on = value.use === 'fresh-copy';

  return (
    <label className="flex items-baseline gap-2 text-body text-ink">
      <input
        type="checkbox"
        checked={on}
        onChange={() => {
          onEditStep({ folder: nextFolder(value) });
        }}
      />
      <span className="flex flex-col gap-0.5">
        <span>Fresh copy of the files</span>
        {/* Zdanie z makiety, słowo w słowo — łącznie z drugą połową. Bez niej ktoś weźmie to
            za piaskownicę bezpieczeństwa, a to jest wyłącznie ochrona przed nadpisywaniem
            plików przez dwa kroki naraz. */}
        <span className={FROM_AGENT}>
          This step gets its own copy so it can&apos;t collide with another step. Not a security
          sandbox.
        </span>
      </span>
    </label>
  );
}

export function StepPanel({
  step,
  agent,
  agents,
  onChooseAgent,
  onCreateAgent,
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
    <>
      <NameRow value={step.name} onEditStep={onEditStep} />

      <WhoDoesThis
        chosen={step.agent}
        agents={agents}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        chip={changed.length > 0 ? <span className={CHIP}>{changed.length} changed</span> : null}
        note="This comes from the agent. Changing it here does not change the agent."
        inherited={noRowOfTheirOwn(changed).map((field) => (
          <div key={field} className="flex items-baseline gap-2">
            {mark(field)}
            {wasUsing(field)}
          </div>
        ))}
      />

      <WhatToDoRow value={step.instructions} onEditStep={onEditStep} />

      <CopiesRow value={step.copies} onEditStep={onEditStep} />

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
    </>
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

export interface PickAnAgentProps {
  step: AgentStep;
  /** Biblioteka agentów. Pusta znaczy „nie ma z czego wybierać" i mówimy to zdaniem. */
  agents: readonly Agent[];
  onChooseAgent: (agentId: string) => void;
  onCreateAgent: () => void;
  onEditStep: (fields: AgentStepFields) => void;
}

/** Panel kroku, który nie ma jeszcze agenta — czyli KAŻDEGO kroku prosto z `＋ Add step`.
 *
 * Dlaczego osobny komponent, a nie siedem wierszy z ukrytą częścią: trzy z siedmiu wierszy
 * `StepPanel` pokazują wartości EFEKTYWNE, a te nie istnieją, dopóki nie ma od kogo dziedziczyć.
 * Wypisanie w nich zer i pustych napisów byłoby ekranem, który mówi nieprawdę o tym, co się
 * stanie po uruchomieniu, a wyszarzenie ich obiecuje ustawienie „na później".
 *
 * CZTERY WIERSZE, NIE DWA — poprawione 2026-08-18. Do tego dnia stały tu Name i Who does this,
 * i to wszystko. `What to do` oraz `How many at once` są polami SAMEGO kroku: nie dziedziczą
 * niczego, nie mają wartości efektywnej i nie ma żadnego powodu, żeby czekały na wybór agenta.
 * Skutek tamtej wersji był zmierzony na dysku właściciela: oba jego pliki mają
 * `"instructions": ""` przy każdym kroku. */
function PickAnAgent({
  step,
  agents,
  onChooseAgent,
  onCreateAgent,
  onEditStep,
}: PickAnAgentProps): ReactElement {
  return (
    <div data-needs-agent className="flex flex-col gap-3">
      <NameRow value={step.name} onEditStep={onEditStep} />

      <WhoDoesThis
        chosen={step.agent}
        agents={agents}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        note="This step waits here until it has someone to do it."
      />

      <WhatToDoRow value={step.instructions} onEditStep={onEditStep} />

      <CopiesRow value={step.copies} onEditStep={onEditStep} />
    </div>
  );
}

export interface PanelForStepProps {
  /** Zaznaczony kafelek — DOWOLNEGO rodzaju. Rozstrzygnięcie, co z nim zrobić, jest niżej. */
  step: Step;
  /** Biblioteka agentów: panel pokazuje wartości efektywne, więc musi znać agenta kroku. */
  agents: readonly Agent[];
  /**
   * Umiejętności, które NAPRAWDĘ leżą w katalogach agentów (`list_skills`).
   *
   * Nazwy, nie obiekty: wiersz Skills zapisuje w kroku listę nazw i niczego więcej o nich nie
   * wie. Pusta lista znaczy „nie ma czego wybierać" i wiersz wtedy nie powstaje.
   */
  skills: readonly string[];
  onChooseAgent: (agentId: string) => void;
  /** Skrót na sekcję Agents — z pozycji `＋ Create a new agent…` i z pustej biblioteki. */
  onCreateAgent: () => void;
  /** Agent jedzie Z POWROTEM do wołającego, bo to tutaj rozwiązuje się `step.agent`
   * (niezmiennik 13). Ekran, który rozwiązywałby go drugi raz u siebie, mógłby rozwiązać
   * inaczej i pokazać wartości efektywne innego agenta niż ten, którego panel nazywa. */
  onEdit: (agent: Agent, edit: Overrides) => void;
  onEditStep: (fields: AgentStepFields) => void;
  onEditCheckpoint: (fields: CheckpointFields) => void;
  onReset: (field: OverridableField) => void;
  onChooseSkills: (choice: SkillChoice) => void;
  /**
   * Ile rund ma powrót wychodzący z tego kroku, albo `null`, gdy żaden z niego nie wychodzi.
   *
   * Liczba, nie strzałka: panel nie ma potrzeby wiedzieć, DOKĄD ten powrót prowadzi, a im mniej
   * dokumentu tu wchodzi, tym mniej jest miejsc, w których panel mógłby coś o nim skłamać.
   */
  wayBack: number | null;
  /** Nowa liczba rund dla tego powrotu. Nie ma jak jej podać, gdy `wayBack` jest `null`. */
  onEditWayBack: (turns: number) => void;
}

/** Jaki panel dostaje zaznaczony kafelek. Trzy odpowiedzi i ani jednego „nic".
 *
 * Kafelek bez panelu jest kafelkiem, którego nie da się skonfigurować — a płótno pozwala
 * postawić go jednym kliknięciem. Dlatego ta funkcja jest CAŁKOWITA: nie ma wejścia, dla
 * którego oddałaby `null`. Dopóki decyzja mieszkała w `editor.tsx` jako warunek
 * `open === undefined || agentOf === undefined`, dwa z trzech wejść dostawały zdanie
 * „Pick a step to see what it was given." — czyli odpowiedź na zupełnie inne pytanie.
 *
 * RAMKA JEST TUTAJ, jedna, od 2026-08-18. Wcześniej każdy z trzech paneli rysował własne
 * `<aside class="w-82 border-l bg-panel p-4">` WEWNĄTRZ `<aside>` szerokości 330 px, którą
 * rysuje `editor.tsx` — czyli dwie ramki, dwa razy padding i poziomy pasek przewijania na
 * każdym otwartym kroku. Ramkę ma teraz ekran (bo to on wie, ile miejsca oddał kolumnie),
 * a panele są jej treścią. */
export function PanelForStep({
  step,
  agents,
  skills,
  onChooseAgent,
  onCreateAgent,
  onEdit,
  onEditStep,
  onEditCheckpoint,
  onReset,
  onChooseSkills,
  wayBack,
  onEditWayBack,
}: PanelForStepProps): ReactElement {
  if (step.kind === 'checkpoint') {
    return (
      <div data-step-panel className="flex flex-col gap-3">
        <CheckpointPanel step={step} onEditStep={onEditCheckpoint} />
      </div>
    );
  }

  const agent = agents.find((one) => one.id === step.agent);
  if (agent === undefined) {
    return (
      <div data-step-panel className="flex flex-col gap-3">
        <PickAnAgent
          step={step}
          agents={agents}
          onChooseAgent={onChooseAgent}
          onCreateAgent={onCreateAgent}
          onEditStep={onEditStep}
        />
      </div>
    );
  }

  return (
    <div data-step-panel className="flex flex-col gap-3">
      <StepPanel
        step={step}
        agent={agent}
        agents={agents}
        onChooseAgent={onChooseAgent}
        onCreateAgent={onCreateAgent}
        onEdit={(edit) => {
          onEdit(agent, edit);
        }}
        onEditStep={onEditStep}
        onReset={onReset}
      />

      {/* Przełącznik własnej kopii plików, w rzędzie przełączników pod siedmioma polami —
          dokładnie tam, gdzie stawia go makieta (linia 620), i z tego samego powodu, dla którego
          stoi tu Skills: to nie jest pole z etykietą, tylko przełącznik. */}
      <FreshCopyRow value={step.folder} onEditStep={onEditStep} />

      {/* Liczba rund powrotu — tylko na kroku, z którego powrót wychodzi. */}
      {wayBack === null ? null : <TriesRow value={wayBack} onEditWayBack={onEditWayBack} />}

      {/* Wiersz Skills, zamontowany PO SIEDMIU wierszach i poza `StepPanel` — patrz nagłówek.
          Przy pustej liście nie powstaje wcale: kiedy w katalogach agentów nie leży ani jedna
          umiejętność, „all" i „none" znaczą dokładnie to samo, a przełącznik między dwoma
          identycznymi skutkami jest kontrolką bez skutku (niezmiennik 16). Wiersza nie ma też
          przy agencie na Codeksie i tę decyzję podejmuje sam komponent. */}
      {skills.length === 0 ? null : (
        <SkillsRow
          mode={SKILL_SUBSETTING}
          runsWith={agent.runsWith}
          available={[...skills]}
          value={step.skills}
          onChoose={onChooseSkills}
        />
      )}
    </div>
  );
}
