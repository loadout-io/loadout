import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';
import { why } from '../../ipc/why';
import { activeWorkspace } from '../../state/workspaces';
import * as Disk from './io';

export type Compatibility = 'exact' | 'adjusted' | 'needs_choice' | 'unsupported';
export type SourceKind =
  'claude' | 'codex' | 'agent_skills' | 'rulesync' | 'open_standard' | 'unknown';
/** Lustro `import::ItemKind`: pięć rodzajów rzeczy, które Loadout umie u siebie postawić. */
export type ItemKind = 'agent' | 'skill' | 'connection' | 'workflow' | 'memory';

export interface SourceItem {
  id: string;
  source?: SourceKind;
  /* Rust wysyła rodzaj ZAWSZE (`import::SourceItem::kind` nie jest `Option`), a od 2026-08-29
   * jest ich pięć i każdy ma swoją nazwę. Pole opcjonalne kazało tu trzymać szóstą nazwę
   * („Other") na wypadek, który nie zachodzi — czyli słowo na ekranie bez rzeczy pod spodem. */
  kind: ItemKind;
  path: string;
  name: string;
  summary: string;
}

export interface Mapping {
  itemId: string;
  compatibility: Compatibility;
  message: string;
}

export type ImportSourceRole = 'definition' | 'behavior' | 'dependency';
export type ImportStatus = 'ready' | 'needs_choice' | 'unsupported' | 'missing_dependencies';

export interface ImportSource {
  provider: SourceKind;
  path: string;
  hash: string;
  role: ImportSourceRole;
}

/** Addytywny model T-78. Stare wektory zostają do czasu, aż będą z niego materializowane. */
export interface ImportItem {
  id: string;
  kind: ItemKind;
  sources: ImportSource[];
  target: string | null;
  dependencies: string[];
  status: ImportStatus;
  statusMessage: string;
  generatedHash: string | null;
}

/** Czego agent ma dotknąć, żeby porównać kopie jednej pozycji.
 *
 * Klucze są nazwami parametrów `compare_import_copies` w `src-tauri/src/ipc.rs` — Tauri
 * dopasowuje argumenty PO NAZWIE, a rozjazd objawia się dopiero pod palcem człowieka.
 */
export interface CompareRequest {
  workspace: string;
  item: string;
  agent: string;
}

/** Druga opinia o kopiach jednej pozycji — lustro `import::compare::Comparison`.
 *
 * `compared` niesie ŚCIEŻKI, które agent naprawdę przeczytał, a nie ich liczbę: człowiek
 * ma rozstrzygnąć konkretne dwa pliki, więc odpowiedź, która ich nie nazywa, jest zdaniem
 * o czymś innym. `keep` to jedna z tych ścieżek albo `null`, kiedy z prozy agenta nie
 * wynika żadna — zgadywanie tutaj byłoby radą, której nikt nie udzielił.
 */
export interface Comparison {
  itemId: string;
  compared: string[];
  said: string;
  keep: string | null;
}

/** Skąd połączenie się wzięło — lustro `connections::Origin`.
 *
 * Brak pola znaczy „plik projektu": tak wygląda każde połączenie zapisane przed 2026-08-22
 * i tak samo rozstrzyga to serde po stronie Rusta. */
export type ConnectionOrigin = 'project' | 'yours-here' | 'yours-everywhere';

export interface ImportedConnection {
  id: string;
  name: string;
  enabled: boolean;
  origin?: ConnectionOrigin;
}

export interface ImportPreview {
  snapshot: { root: string; items: SourceItem[] };
  draft: {
    sourceHashes: Record<string, string>;
    items: ImportItem[];
    agents: Array<{ id: string; name: string }>;
    skills: Array<{ name: string }>;
    connections: ImportedConnection[];
    workflows: Array<{
      id: string;
      name: string;
      description?: string;
      steps?: Array<{
        kind: 'agent' | 'check' | 'checkpoint';
        id: string;
        name: string;
        command?: string;
      }>;
    }>;
    report: { mappings: Mapping[] };
  };
}

export interface ApplyRequest {
  workspace: string;
  expectedSourceHashes: Record<string, string>;
  enableConnections: string[];
  /** Pole przejściowe dla starego ekranu; T-78 zastępuje je dwiema decyzjami poniżej. */
  leaveOut?: string[];
  excludedItems?: string[];
  withoutBehavior?: string[];
}

export interface ImportReceipt {
  id: string;
  written: string[];
  enabledConnections: string[];
}

export interface ImportIo {
  scanSetup(workspace: string): Promise<ImportPreview>;
  applySetup(request: ApplyRequest): Promise<ImportReceipt>;
  /** `null` znaczy „człowiek to zatrzymał" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
   *  wiersz wraca wtedy do swojego pytania, bez odpowiedzi i bez zdania o awarii. */
  compareCopies(ask: CompareRequest): Promise<Comparison | null>;
  stopComparing(): Promise<void>;
}

export interface ImportSetupProps {
  onClose: () => void;
  onImported: () => void;
  io?: ImportIo;
  initialPreview?: ImportPreview;
  /** Zapisani agenci — to jeden z nich porówna kopie. Pusta lista znaczy „nie ma kogo
   *  o to poprosić", i wtedy zamiast przycisku stoi zdanie (niezmiennik 16). */
  agents?: Array<{ id: string; name: string }>;
}

/* OKNO MODALA JEST NIEPRZEJRZYSTE, i to jest naprawa, nie preferencja (2026-08-22).
 *
 * Do tego dnia stało tu `bg-panel`, a `--color-panel` to `rgba(255,255,255,0.045)` — bielą-alfa
 * podnosi się powierzchnię LEŻĄCĄ NA TLE APLIKACJI, nie zasłania nią cudzej treści. Nad ekranem
 * Agentów, przy tle za modalem na 72%, ekran spod spodu przebijał przez okno: nazwy agentów
 * i ich opisy nachodziły na wiersze importu i nie dało się tego czytać (zrzut właściciela).
 *
 * `--overlay` (`#1b1b24`) jest tokenem, który DESIGN §6 wymienia przy `modal` wprost, ze słowem
 * „nieprzejrzysty" w nawiasie — więc to nie jest nowy kolor ani odstępstwo od makiety, tylko
 * użycie tego, co makieta na to miejsce przewidziała. Ta sama pomyłka stała w czterech modalach
 * naraz i wszystkie cztery są poprawione razem: jedna klasa, jedna przyczyna. */
const BACKDROP = 'fixed inset-0 z-20 flex items-center justify-center bg-bg/72 p-6';
/* BEZ DOLNEGO WYPEŁNIENIA, i to jest część paska przy dole, a nie oszczędność (2026-08-29).
 *
 * Okno jest własnym pasem przewijania, więc treść przewija się TAKŻE przez jego wypełnienie.
 * Dopóki `p-6` zostawiało 24 px pod przyklejonym paskiem akcji, spod paska wyzierała kolejna
 * ścieżka z listy „Proposed files" — zależnie od tego, którą krawędź `position: sticky` weźmie
 * za swoją. Dolne 24 px oddaje więc sam pasek (`pb-6`), a ekran bez planu — akapit pod spodem. */
const WINDOW =
  'flex max-h-full w-full max-w-240 flex-col gap-4 overflow-auto rounded-lg border border-line-strong bg-overlay px-6 pt-6';
const BUTTON = 'h-8 rounded-sm border border-line px-3 text-ui text-body';
const ORIGIN = 'text-label text-muted';

/** Zdanie o pochodzeniu połączenia — mówi, KTO JE WIDZI, nie w którym pliku leży.
 *
 * Ścieżka pliku odpowiadałaby na to samo pytanie okrężnie i tylko komuś, kto zna trzy zakresy
 * Claude Code na pamięć. „Just you" kontra „in the project" rozstrzyga to jednym spojrzeniem. */
function whereFrom(origin: ConnectionOrigin | undefined): string {
  if (origin === 'yours-here') return 'just you, in this project';
  if (origin === 'yours-everywhere') return 'just you, everywhere';
  return 'in the project';
}
const PRIMARY = 'h-8 rounded-sm bg-accent px-3 text-ui text-bg';

const STATUS: Readonly<Record<Compatibility, string>> = {
  exact: 'Exact',
  adjusted: 'Adjusted',
  needs_choice: 'Needs a choice',
  unsupported: "Can't be reproduced",
};

const SOURCES: Readonly<Record<SourceKind, string>> = {
  claude: 'Claude',
  codex: 'Codex',
  agent_skills: 'Agent Skills',
  rulesync: 'Rulesync',
  open_standard: 'Project file',
  unknown: 'Project file',
};

/* JEDNO SŁOWO NA JEDNĄ RZECZ, i tym słowem jest „Workflow" (2026-08-29).
 *
 * Do tego dnia stało tu `workflow: 'Routine'` — nazwa, która w całym produkcie nie istniała
 * nigdzie indziej: sekcja w pasku nazywa się Workflows, licznik nad tabelą nazywa się
 * Workflows, pliki lądują w `workflows/`. Kolumna „Type" mówiła więc „Routine" trzy cale pod
 * licznikiem mówiącym „Workflows", o tej samej rzeczy (niezmiennik 13). */
const KINDS: Readonly<Record<ItemKind, string>> = {
  agent: 'Agent',
  skill: 'Skill',
  connection: 'Connection',
  workflow: 'Workflow',
  memory: 'Note',
};

const ITEM_STATUS: Readonly<Record<ImportStatus, string>> = {
  ready: 'Ready',
  needs_choice: 'Needs a choice',
  unsupported: "Can't be reproduced",
  missing_dependencies: 'Missing dependencies',
};

type InventoryView = 'all' | 'ready' | 'attention';

function sourceLabel(item: SourceItem): string {
  if (item.source !== undefined) return SOURCES[item.source];
  if (item.path.startsWith('.claude/')) return SOURCES.claude;
  if (item.path.startsWith('.codex/')) return SOURCES.codex;
  if (item.path.startsWith('.agents/')) return SOURCES.agent_skills;
  if (item.path.startsWith('.rulesync/')) return SOURCES.rulesync;
  return SOURCES.unknown;
}

function kindLabel(item: SourceItem): string {
  return KINDS[item.kind];
}

/** Czy ta pozycja czeka na rozstrzygnięcie człowieka — i tylko taka daje się oddać agentowi.
 *
 * `unsupported` jest tu razem z `needs_choice` z rozmysłem: `Can't be reproduced` zostaje
 * widoczne DOPÓKI takiej analizy nie było (docs/PLAN.md §6d), więc to jest dokładnie ten
 * wiersz, przy którym druga opinia ma co powiedzieć. Sam status nie rusza się od niej ani
 * o jotę — porównanie doradza, nie rozstrzyga. */
function awaitsAChoice(item: ImportItem): boolean {
  return item.status === 'needs_choice' || item.status === 'unsupported';
}

/** „a", „a and b", „a, b and c" — ścieżki wyliczone tak, jak wylicza się je w zdaniu.
 *
 * Wprost, a nie `join(', ')`: człowiek czyta tu, ILE plików agent przeczytał, i przecinek
 * przed ostatnim z dwóch wygląda jak lista ucięta w połowie. */
function andThen(paths: readonly string[]): string {
  if (paths.length < 2) return paths.join('');
  return `${paths.slice(0, -1).join(', ')} and ${paths[paths.length - 1] ?? ''}`;
}

/** Zdania agenta o kopiach — przy tej pozycji, której dotyczyły.
 *
 * Cztery rzeczy, w tej kolejności: co przeczytał, co powiedział, co proponuje i KTO decyduje.
 * Ostatnie zdanie jest stałe i nie ma prawa zniknąć: agent doradza, człowiek importuje — ta
 * sama granica, którą trzyma weryfikator (AGENTS.md §2). Ptaszek `Import` i status wiersza
 * zostają tam, gdzie były; ten blok wyłącznie dopisuje tekst. */
function SecondOpinion({ said }: { said: Comparison }): ReactElement {
  return (
    <span className="mt-2 block border-l border-line pl-2">
      <span className="block text-note text-muted">{`An agent read ${andThen(said.compared)}.`}</span>
      <span className="block text-note text-body">{said.said}</span>
      {said.keep === null ? null : (
        <span className="block text-note text-body">
          {`It suggests keeping the copy from ${said.keep}.`}
        </span>
      )}
      <span className="block text-note text-muted">This is advice. You choose what to import.</span>
    </span>
  );
}

function typedExcludedIn(preview: ImportPreview): string[] {
  return preview.draft.items.filter((item) => item.status !== 'ready').map((item) => item.id);
}

function hasDependency(
  preview: ImportPreview,
  dependency: string,
  excludedItems: readonly string[],
  enabledConnections: readonly string[],
): boolean {
  const divider = dependency.indexOf(':');
  if (divider < 1) return false;
  const kind = dependency.slice(0, divider);
  const name = dependency.slice(divider + 1).toLocaleLowerCase();
  if (kind === 'connection') {
    return preview.draft.connections.some(
      (connection) =>
        enabledConnections.includes(connection.id) &&
        (connection.id.toLocaleLowerCase() === name ||
          connection.name.toLocaleLowerCase() === name),
    );
  }
  if (kind === 'skill') {
    const exists = preview.draft.skills.some((skill) => skill.name.toLocaleLowerCase() === name);
    const selected = preview.draft.items
      .filter((item) => item.kind === 'skill')
      .some(
        (item) =>
          !excludedItems.includes(item.id) &&
          (item.target?.toLocaleLowerCase().includes(`/skills/${name}/`) === true ||
            item.target?.toLocaleLowerCase().startsWith(`skills/${name}/`) === true),
      );
    return exists && selected;
  }
  if (kind === 'agent') {
    const exists = preview.draft.agents.some(
      (agent) => agent.id.toLocaleLowerCase() === name || agent.name.toLocaleLowerCase() === name,
    );
    const selected = preview.draft.items
      .filter((item) => item.kind === 'agent')
      .some(
        (item) =>
          !excludedItems.includes(item.id) &&
          item.target?.toLocaleLowerCase().startsWith(`agents/${name}`) === true,
      );
    return exists && selected;
  }
  return false;
}

function typedBlockers(
  preview: ImportPreview,
  excludedItems: readonly string[],
  withoutBehavior: readonly string[],
  enabledConnections: readonly string[],
): number {
  return preview.draft.items.filter((item) => {
    if (excludedItems.includes(item.id)) return false;
    if (item.status === 'unsupported') return true;
    if (item.status === 'needs_choice' && !withoutBehavior.includes(item.id)) return true;
    return item.dependencies.some(
      (dependency) => !hasDependency(preview, dependency, excludedItems, enabledConnections),
    );
  }).length;
}

function blockers(preview: ImportPreview, leaveOut: readonly string[]): number {
  return preview.draft.report.mappings.filter(
    (mapping) =>
      (mapping.compatibility === 'unsupported' || mapping.compatibility === 'needs_choice') &&
      !leaveOut.includes(mapping.itemId),
  ).length;
}

/** Pozycje, których Loadout nie umie wnieść: nieobsługiwane i te, które wymagają wyboru. */
function unresolvedIn(preview: ImportPreview): string[] {
  return preview.draft.report.mappings
    .filter(
      (mapping) =>
        mapping.compatibility === 'unsupported' || mapping.compatibility === 'needs_choice',
    )
    .map((mapping) => mapping.itemId);
}

export function ImportSetup({
  onClose,
  onImported,
  io = Disk,
  initialPreview,
  agents = [],
}: ImportSetupProps): ReactElement {
  /* Import dotyczy projektu otwartego w bocznym menu, więc zaczyna od tego samego, jedynego
   * źródła prawdy co Run i Skills. Puste pole zostaje wyłącznie wtedy, gdy człowiek nie wybrał
   * jeszcze żadnego workspace; wpisanie ścieżki ręcznie nadal pozwala zeskanować inny projekt. */
  const [workspace, setWorkspace] = useState(
    initialPreview?.snapshot.root ?? activeWorkspace()?.folder ?? '',
  );
  const [preview, setPreview] = useState<ImportPreview | null>(initialPreview ?? null);
  const [enabled, setEnabled] = useState<string[]>([]);
  /* POZYCJE NIE DO WNIESIENIA SĄ POMINIĘTE OD RAZU, a nie po 68 kliknięciach.
   *
   * 2026-08-22 — ZGŁOSZENIE WŁAŚCICIELA: „trzeba kliknąć każdy element po kolei, żeby
   * zaimportować, bo inne, których nie da się zaimportować, wtedy nas blokują". Miał rację i to
   * nie było drobiazgiem: jedynym rozstrzygnięciem, jakie ten ekran w ogóle oferuje dla pozycji
   * `needs_choice`, jest „Skip" — więc żądanie wyboru było żądaniem kliknięcia jedynej możliwej
   * odpowiedzi, raz na pozycję. Przy 68 pozycjach import po prostu nie odbywał się nigdy.
   *
   * Domyślne pominięcie NIE UKRYWA niczego: każda taka pozycja stoi w tabeli z powodem i ze
   * swoim zaznaczonym „Skip", a stopka mówi, ile ich jest. Człowiek, który chce jedną z nich
   * wnieść mimo wszystko, odznacza jej pole — i wtedy dopiero import czeka na rozstrzygnięcie,
   * bo o to właśnie poprosił. */
  const [leaveOut, setLeaveOut] = useState<string[]>(
    initialPreview === undefined ? [] : unresolvedIn(initialPreview),
  );
  const [excludedItems, setExcludedItems] = useState<string[]>(
    initialPreview === undefined ? [] : typedExcludedIn(initialPreview),
  );
  const [withoutBehavior, setWithoutBehavior] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [saved, setSaved] = useState<ImportReceipt | null>(null);
  const [inventoryView, setInventoryView] = useState<InventoryView>('all');
  /* Kogo pytamy o kopie. Pusty napis znaczy „człowiek jeszcze nie wybrał", a nie „nikogo":
   * lista agentów przyjeżdża propsem i bywa pusta w chwili pierwszego renderu, więc wybór
   * domyślny liczy się przy każdym renderze ([`whoCompares`] niżej), a nie raz w `useState`. */
  const [chosenAgent, setChosenAgent] = useState('');
  /* Identyfikator pozycji, przy której agent PRACUJE TERAZ. Jeden naraz, tak jak po tamtej
   * stronie granicy (`commands::import::Comparing`) — drugie pytanie jest tam odmową, więc
   * dwa wiersze mówiące „porównuje teraz" byłyby zdaniem o czymś, co się nie dzieje. */
  const [comparing, setComparing] = useState<string | null>(null);
  const [comparisons, setComparisons] = useState<Record<string, Comparison>>({});
  const whoCompares = chosenAgent === '' ? (agents[0]?.id ?? '') : chosenAgent;
  const hasTypedItems = preview !== null && preview.draft.items.length > 0;
  const selectedTypedItems =
    preview?.draft.items.filter((item) => !excludedItems.includes(item.id)) ?? [];
  const blocked =
    preview === null
      ? 0
      : hasTypedItems
        ? typedBlockers(preview, excludedItems, withoutBehavior, enabled)
        : blockers(preview, leaveOut);
  const hasItems = preview !== null && preview.snapshot.items.length > 0;
  const unresolved = preview === null ? [] : unresolvedIn(preview);
  /* Ile pozycji naprawdę zostanie poza importem — liczone z listy pominięć, nie z `unresolved`:
   * człowiek mógł którąś odznaczyć i wtedy ona już nie „zostaje poza", tylko blokuje. */
  const leftOut = unresolved.filter((id) => leaveOut.includes(id)).length;
  const mappings = new Map(
    preview?.draft.report.mappings.map((mapping) => [mapping.itemId, mapping]) ?? [],
  );
  const sourceItems = new Map(preview?.snapshot.items.map((item) => [item.id, item]) ?? []);
  const visibleTypedItems =
    preview?.draft.items.filter((item) => {
      const ready = item.status === 'ready';
      return inventoryView === 'all' || (inventoryView === 'ready' ? ready : !ready);
    }) ?? [];
  const visibleItems = hasTypedItems
    ? []
    : (preview?.snapshot.items.filter((item) => {
        const compatibility = mappings.get(item.id)?.compatibility ?? 'unsupported';
        const ready = compatibility === 'exact' || compatibility === 'adjusted';
        return inventoryView === 'all' || (inventoryView === 'ready' ? ready : !ready);
      }) ?? []);
  /* Wybór agenta stoi na ekranie tylko wtedy, kiedy jest co porównywać: pole nad tabelą,
   * w której każda pozycja jest gotowa, byłoby pytaniem bez skutku (niezmiennik 16). */
  const anyAwaitsAChoice = preview?.draft.items.some(awaitsAChoice) ?? false;

  const scan = (event: FormEvent): void => {
    event.preventDefault();
    if (workspace.trim() === '') return;
    setBusy(true);
    setRefusal(null);
    setSaved(null);
    void io
      .scanSetup(workspace.trim())
      .then((next) => {
        setPreview(next);
        setEnabled([]);
        /* Świeży skan wraca z domyślnym pominięciem, tak samo jak pierwsze otwarcie: inaczej
         * jedno kliknięcie „Scan" cofałoby ekran do stanu, w którym import jest zablokowany. */
        setLeaveOut(unresolvedIn(next));
        setExcludedItems(typedExcludedIn(next));
        setWithoutBehavior([]);
        /* Odpowiedzi z poprzedniego skanu odchodzą razem z nim: zdanie o dwóch kopiach
         * jest zdaniem o KONKRETNYCH plikach, a te po ponownym skanie mogą być inne. */
        setComparisons({});
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not inspect that folder.'));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  const apply = (): void => {
    if (preview === null || !hasItems || blocked > 0) return;
    setBusy(true);
    setRefusal(null);
    void io
      .applySetup({
        workspace: preview.snapshot.root,
        expectedSourceHashes: preview.draft.sourceHashes,
        enableConnections: enabled,
        leaveOut: hasTypedItems ? [] : leaveOut,
        excludedItems: hasTypedItems ? excludedItems : [],
        withoutBehavior: hasTypedItems ? withoutBehavior : [],
      })
      .then((saved) => {
        setSaved(saved);
        onImported();
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not save that setup.'));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  /* Jedno pytanie do agenta o kopie JEDNEJ pozycji.
   *
   * `setBusy` NIE JEST tu wołane z rozmysłem: `busy` blokuje Scan i Import, a porównanie nie
   * zmienia planu ani o wiersz — człowiek ma w jego trakcie nadal móc odznaczyć ptaszek albo
   * wnieść resztę. Jedyne, co ta droga zmienia, to tekst przy pozycji. */
  const compare = (item: ImportItem): void => {
    if (preview === null || whoCompares === '' || comparing !== null) return;
    setComparing(item.id);
    setRefusal(null);
    void io
      .compareCopies({ workspace: preview.snapshot.root, item: item.id, agent: whoCompares })
      .then((said) => {
        /* `null` znaczy „człowiek nacisnął Stop" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
         * wiersz wraca wtedy do swojego pytania, bez odpowiedzi i bez zdania o awarii. */
        if (said === null) return;
        setComparisons((now) => ({ ...now, [item.id]: said }));
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not ask an agent about those copies.'));
      })
      .finally(() => {
        setComparing(null);
      });
  };

  /* „Stop": zatrzymuje agenta, który porównuje teraz. Dowód, że zszedł, wraca odpowiedzią
   * na `compareCopies` — czyli tym samym wywołaniem, na które ten ekran już czeka; drugą
   * drogą na ten sam fakt byłoby drugie miejsce, w którym on mieszka (niezmiennik 13). */
  const stopComparing = (): void => {
    void io.stopComparing().catch((error: unknown) => {
      setRefusal(why(error, 'Loadout could not stop that agent.'));
    });
  };

  return (
    <div className={BACKDROP}>
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-setup-title"
        className={WINDOW}
      >
        <div className="flex items-center gap-3">
          <div>
            <h2 id="import-setup-title" className="text-heading text-ink">
              Import setup
            </h2>
            <p className="text-note text-muted">
              Turn project agents, skills, connections, workflows, and notes into Loadout files.
            </p>
            {/* CO ZOSTAJE W PROJEKCIE, powiedziane wprost (2026-08-29).
                Do tego dnia hooki, reguły i ustawienia cudzej aplikacji stały na tej liście
                jako wiersze z pytaniem bez odpowiedzi. Zniknęły, bo Loadout nie ma dla nich
                ani sekcji, ani wykonawcy — ale zniknęły też BEZ SŁOWA, a milczenie po takiej
                zmianie czyta się jak przeoczenie. Drugie zdanie mówi też, dlaczego to nie jest
                strata: krok startuje z `current_dir` w folderze projektu, więc `CLAUDE.md`
                i `.claude/rules/` czyta sam agent. Zmierzone sondą tego samego dnia — także
                pod `--setting-sources ""`, wbrew temu, co mówi dokumentacja vendora. */}
            <p className="text-note text-muted">
              Hooks, project rules, and app settings stay in the project — agents read them from the
              project folder.
            </p>
          </div>
          <button type="button" className={`ml-auto ${BUTTON}`} onClick={onClose}>
            Close
          </button>
        </div>

        <form className="flex items-end gap-2" onSubmit={scan}>
          <label className="flex min-w-0 flex-1 flex-col gap-1 text-label text-muted">
            Project folder
            <input
              value={workspace}
              onChange={(event) => {
                setWorkspace(event.target.value);
              }}
              placeholder="/Users/you/project"
              className="h-9 rounded-sm border border-line bg-well px-3 text-body text-ink"
            />
          </label>
          <button type="submit" disabled={busy || workspace.trim() === ''} className={PRIMARY}>
            Scan
          </button>
        </form>

        {refusal === null ? null : (
          <p role="alert" className="border border-fail-edge bg-fail-soft p-3 text-body text-fail">
            {refusal}
          </p>
        )}
        {saved === null ? null : (
          <p
            role="status"
            className="text-body text-ink"
          >{`${String(saved.written.length)} files imported.`}</p>
        )}

        {preview === null ? (
          <p className="pb-6 text-body text-muted">
            Scan reads setup files only. It does not run hooks, skills, agents, or connections.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-5 gap-2 border-y border-line py-3 text-center">
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'agent').length
                    : preview.draft.agents.length}
                </b>
                <small className="text-muted">Agents</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'skill').length
                    : preview.draft.skills.length}
                </b>
                <small className="text-muted">Skills</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'connection').length
                    : preview.draft.connections.length}
                </b>
                <small className="text-muted">Connections</small>
              </span>
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'workflow').length
                    : preview.draft.workflows.length}
                </b>
                <small className="text-muted">Workflows</small>
              </span>
              {/* PIĄTY LICZNIK, bo notatki też się importują (2026-08-29).
                  Na zrzucie właściciela plan wnosił 16 notatek i nie było ich w tym wierszu
                  ani razu: cztery liczniki nad tabelą pokrywały 28 z 49 pozycji. Wektor
                  sprzed T-78 nie ma notatek w ogóle, więc dla niego uczciwą liczbą jest 0 —
                  ten kształt planu powstał, zanim import notatek istniał. */}
              <span>
                <b className="block text-heading text-ink">
                  {hasTypedItems
                    ? selectedTypedItems.filter((item) => item.kind === 'memory').length
                    : 0}
                </b>
                <small className="text-muted">Notes</small>
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-label text-muted">Show</span>
              {(
                [
                  ['all', 'All'],
                  ['ready', 'Ready'],
                  ['attention', 'Needs attention'],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className={inventoryView === value ? PRIMARY : BUTTON}
                  onClick={() => {
                    setInventoryView(value);
                  }}
                >
                  {label}
                </button>
              ))}
              {/* KTO PORÓWNA KOPIE — jedno pole na całą tabelę, nie jedno na wiersz.
                  Wybór agenta jest tą samą odpowiedzią dla każdej pozycji, która o niego
                  prosi, a siedemnaście takich wierszy to siedemnaście identycznych list
                  do rozwinięcia. Znika razem z powodem: bez ani jednej pozycji czekającej
                  na rozstrzygnięcie nie ma czego porównywać. */}
              {!anyAwaitsAChoice || agents.length === 0 ? null : (
                <label className="ml-auto flex items-center gap-2 text-label text-muted">
                  Who should compare them?
                  <select
                    value={whoCompares}
                    onChange={(event) => {
                      setChosenAgent(event.target.value);
                    }}
                    className="h-8 rounded-sm border border-line bg-well px-2 text-body text-ink"
                  >
                    {agents.map((agent) => (
                      <option key={agent.id} value={agent.id}>
                        {agent.name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </div>
            {/* TABELA DOSTAJE WYSOKOŚĆ JAKO PIERWSZA, nie jako ostatnia (2026-08-29).

                ZGŁOSZENIE WŁAŚCICIELA, ze zrzutu ekranu: skan projektu `meetnotes` znalazł
                86 pozycji i cała tabela zniknęła — została po niej kreska. Stało za tym samo
                `min-h-0`, które tu wcześniej było: okno jest kolumną fleksa z `max-h-full`,
                więc kiedy treść przerasta okno, flexbox ścieśnia dzieci — a ścisnąć się
                poniżej własnej treści pozwalało TYLKO to jedno. Cały nadmiar rosnącej listy
                „Proposed files" szedł więc z tabeli, aż do zera pikseli. Filtr sterował wtedy
                listą, której nie widać, a 37 pominiętych pozycji nie dało się ani obejrzeć,
                ani odznaczyć — czyli jedyna rzecz, po którą się to okno otwiera.

                `flex-1` oddaje jej wolne miejsce jako pierwszej, `min-h-40` stawia podłogę,
                poniżej której już nikt jej nie ścieśni. Dowodzi tego
                `e2e/tests/import-list-stays-visible.spec.ts`, w prawdziwym chromium: układu
                nie liczy ani czysty moduł, ani `renderToStaticMarkup`, więc obie te drogi
                dałyby zielone na kodzie, na którym właściciel widział pusty ekran. */}
            <div
              data-import-items
              className="min-h-40 flex-1 overflow-auto rounded-md border border-line"
            >
              <table className="w-full table-fixed border-collapse text-left">
                <thead className="sticky top-0 bg-panel text-label text-muted">
                  <tr>
                    <th className="w-2/5 px-3 py-2 font-normal">Item</th>
                    <th className="w-24 px-3 py-2 font-normal">Type</th>
                    <th className="w-28 px-3 py-2 font-normal">Source</th>
                    <th className="w-36 px-3 py-2 font-normal">Status</th>
                    <th className="w-24 px-3 py-2 font-normal">Include</th>
                  </tr>
                </thead>
                <tbody>
                  {hasTypedItems
                    ? visibleTypedItems.map((item) => {
                        const sourceItem = sourceItems.get(item.id);
                        /* KAŻDA definicja, nie pierwsza z brzegu (2026-08-29). Odkąd jeden
                           wiersz scala kopie z kilku aplikacji, pokazanie jednej ścieżki
                           przemilczałoby to, że decyzja dotyczy ich obu. */
                        const definitions = item.sources.filter(
                          (source) => source.role === 'definition',
                        );
                        const definition = definitions[0] ?? item.sources[0];
                        /* Do lokalnej stałej, nie w JSX: pod `noUncheckedIndexedAccess` odczyt
                           z mapy daje `Comparison | undefined`, a zawężenie po `item.id`
                           (odczyt pola, nie stała) nie przeżywa granicy wyrażenia. */
                        const compared = comparisons[item.id];
                        return (
                          <tr key={item.id} className="border-t border-line align-top">
                            <td className="px-3 py-2">
                              <b className="block truncate text-body text-ink">
                                {sourceItem?.name ?? item.target ?? item.id}
                              </b>
                              {(definitions.length > 0 ? definitions : item.sources).map(
                                (source) => (
                                  <span
                                    key={source.path}
                                    className="block truncate font-mono text-meta text-muted"
                                  >
                                    {source.path}
                                  </span>
                                ),
                              )}
                              <span className="block text-note text-body">
                                {item.statusMessage}
                              </span>
                              {item.target === null ? null : (
                                <span className="block font-mono text-meta text-muted">
                                  Target: {item.target}
                                </span>
                              )}
                              {item.dependencies.length === 0 ? null : (
                                <span className="block text-note text-muted">
                                  Requires: {item.dependencies.join(', ')}
                                </span>
                              )}
                              {/* DRUGA OPINIA STOI PRZY POZYCJI, KTÓREJ DOTYCZY (2026-08-29).
                                  Adapter sam prosi o agenta („Let an agent compare them before
                                  import.") i do dziś nikt go nie wołał: jedyną odpowiedzią tego
                                  ekranu dla siedemnastu takich wierszy było pominięcie. Wiersz
                                  niesie już OBIE ścieżki — pytanie jest o to, czym te kopie się
                                  różnią, więc odpowiedź nie ma prawa wylądować nigdzie indziej
                                  niż w tej samej komórce. Statusu, ptaszka `Import` ani liczników
                                  nad tabelą ta droga nie dotyka ani o jotę. */}
                              {!awaitsAChoice(item) ? null : (
                                <span className="mt-2 block">
                                  {agents.length === 0 ? (
                                    /* Nie ma kogo poprosić, więc nie ma tu przycisku: kontrolka
                                       bez skutku jest gorsza od zdania (niezmiennik 16). */
                                    <span className="block text-note text-muted">
                                      Save an agent first, then it can compare copies for you.
                                    </span>
                                  ) : comparing === item.id ? (
                                    <>
                                      <span className="block text-note text-body">
                                        An agent is comparing the copies now.
                                      </span>
                                      <button
                                        type="button"
                                        data-stop-comparing
                                        className={`mt-1 ${BUTTON}`}
                                        onClick={stopComparing}
                                      >
                                        Stop
                                      </button>
                                    </>
                                  ) : (
                                    <button
                                      type="button"
                                      data-compare-copies={item.id}
                                      disabled={comparing !== null}
                                      className={BUTTON}
                                      onClick={() => {
                                        compare(item);
                                      }}
                                    >
                                      {/* Jedna kopia to nie jest wybór między kopiami, więc
                                          i pytanie brzmi inaczej: to są te dwa skille nie do
                                          odtworzenia i dwie ceremonie bez wykonalnej treści. */}
                                      {definitions.length > 1
                                        ? 'Compare the copies'
                                        : 'Explain this'}
                                    </button>
                                  )}
                                  {compared === undefined ? null : (
                                    <SecondOpinion said={compared} />
                                  )}
                                </span>
                              )}
                            </td>
                            <td className="px-3 py-2 text-body text-ink">{KINDS[item.kind]}</td>
                            <td className="px-3 py-2 text-body text-muted">
                              {definition === undefined
                                ? SOURCES.unknown
                                : SOURCES[definition.provider]}
                            </td>
                            <td className="px-3 py-2 text-body text-muted">
                              {ITEM_STATUS[item.status]}
                            </td>
                            <td className="px-3 py-2">
                              <label className="flex items-center gap-2 text-body text-ink">
                                <input
                                  type="checkbox"
                                  aria-label="Import this item"
                                  checked={!excludedItems.includes(item.id)}
                                  onChange={(event) => {
                                    setExcludedItems((now) =>
                                      event.target.checked
                                        ? now.filter((id) => id !== item.id)
                                        : [...now, item.id],
                                    );
                                  }}
                                />
                                Import
                              </label>
                              {item.status !== 'needs_choice' ? null : (
                                <label className="mt-2 flex items-start gap-2 text-note text-ink">
                                  <input
                                    type="checkbox"
                                    aria-label="Import without this behavior"
                                    checked={withoutBehavior.includes(item.id)}
                                    onChange={(event) => {
                                      setWithoutBehavior((now) =>
                                        event.target.checked
                                          ? [...now, item.id]
                                          : now.filter((id) => id !== item.id),
                                      );
                                    }}
                                  />
                                  Without behavior
                                </label>
                              )}
                            </td>
                          </tr>
                        );
                      })
                    : visibleItems.map((item) => {
                        const mapping = mappings.get(item.id);
                        const unresolvedItem =
                          mapping?.compatibility === 'needs_choice' ||
                          mapping?.compatibility === 'unsupported';
                        return (
                          <tr key={item.id} className="border-t border-line align-top">
                            <td className="px-3 py-2">
                              <b className="block truncate text-body text-ink">{item.name}</b>
                              <span className="block truncate font-mono text-meta text-muted">
                                {item.path}
                              </span>
                              <span className="block text-note text-body">
                                {mapping?.message ?? item.summary}
                              </span>
                            </td>
                            <td className="px-3 py-2 text-body text-ink">{kindLabel(item)}</td>
                            <td className="px-3 py-2 text-body text-muted">{sourceLabel(item)}</td>
                            <td className="px-3 py-2 text-body text-muted">
                              {mapping === undefined
                                ? "Can't be reproduced"
                                : STATUS[mapping.compatibility]}
                            </td>
                            <td className="px-3 py-2">
                              {unresolvedItem ? (
                                <label className="flex items-center gap-2 text-body text-ink">
                                  <input
                                    type="checkbox"
                                    aria-label={
                                      mapping.compatibility === 'unsupported'
                                        ? 'Leave this item out of the import'
                                        : 'Import without this behavior'
                                    }
                                    checked={leaveOut.includes(item.id)}
                                    onChange={(event) => {
                                      setLeaveOut((now) =>
                                        event.target.checked
                                          ? [...now, item.id]
                                          : now.filter((id) => id !== item.id),
                                      );
                                    }}
                                  />
                                  Skip
                                </label>
                              ) : (
                                <span className="text-body text-muted">Yes</span>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                </tbody>
              </table>
            </div>
            {preview.draft.connections.length === 0 ? null : (
              <fieldset className="flex flex-col gap-2 border-t border-line pt-3">
                <legend className="flex w-full items-center gap-3 text-subhead text-ink">
                  Connections stay off unless you enable them
                  {/* JEDNO KLIKNIĘCIE ZAMIAST N, i to jest cała treść tego przycisku (2026-08-22).
                      Projekt z siedmioma serwerami to dziś siedem ptaszków, a każdy z nich jest tą
                      samą decyzją. Przycisk NIE zmienia reguły — nadal to człowiek włącza, nadal
                      widzi każdą pozycję z osobna i nadal może odznaczyć. */}
                  <button
                    type="button"
                    data-enable-all
                    className={`${BUTTON} ml-auto`}
                    disabled={enabled.length === preview.draft.connections.length}
                    onClick={() => {
                      setEnabled(preview.draft.connections.map((one) => one.id));
                    }}
                  >
                    Turn them all on
                  </button>
                </legend>
                {preview.draft.connections.map((connection) => (
                  <label key={connection.id} className="flex items-center gap-2 text-body text-ink">
                    <input
                      type="checkbox"
                      checked={enabled.includes(connection.id)}
                      onChange={(event) => {
                        setEnabled((now) =>
                          event.target.checked
                            ? [...now, connection.id]
                            : now.filter((id) => id !== connection.id),
                        );
                      }}
                    />
                    {connection.name}
                    {/* SKĄD TO JEST, przy nazwie i po cichu. Człowiek stojący nad tą listą pyta
                        o jedno: czy to ustawienie zespołu, czy moje własne — a od 2026-08-22 na
                        liście stoją obie rodzaje naraz. Bez tego zdania `linear-server` z twojej
                        prywatnej konfiguracji wygląda identycznie jak `context7` z repo. */}
                    <span className={ORIGIN}>{whereFrom(connection.origin)}</span>
                  </label>
                ))}
              </fieldset>
            )}
            {/* SEKCJA „PROPOSED FILES" ZNIKŁA, bo mówiła to samo dwa razy (2026-08-29).
                Jej lista ścieżek powtarzała wiersz `Target:` stojący przy każdej pozycji,
                a jej podsumowanie powtarzało liczniki nad tabelą — jeden fakt w trzech żywych
                miejscach przy limicie 1 (niezmiennik 13). To ona rosła bez sufitu i to ona
                zabierała tabeli całą wysokość okna, aż tabela zniknęła z ekranu. */}
            <div className="sticky bottom-0 z-10 -mx-6 flex items-center gap-3 border-t border-line bg-overlay px-6 pt-3 pb-6">
              {hasTypedItems || unresolved.length === 0 ? null : (
                <button
                  type="button"
                  className={BUTTON}
                  disabled={unresolved.every((id) => leaveOut.includes(id))}
                  onClick={() => {
                    setLeaveOut(unresolved);
                  }}
                >
                  Leave out all unresolved items
                </button>
              )}
              <p className="text-note text-muted">
                {!hasItems
                  ? 'No setup files were found in this project.'
                  : blocked > 0
                    ? hasTypedItems
                      ? `${String(blocked)} item(s) still need attention.`
                      : `${String(blocked)} item(s) need a choice. Tick Skip to leave them out.`
                    : hasTypedItems
                      ? excludedItems.length === 0
                        ? 'Ready to import.'
                        : `Ready to import. ${String(excludedItems.length)} item(s) will not be imported.`
                      : leftOut === 0
                        ? 'Ready to import.'
                        : `Ready to import. ${String(leftOut)} item(s) will be left out.`}
              </p>
              <button
                type="button"
                data-import-now
                disabled={busy || !hasItems || blocked > 0}
                className={`ml-auto ${PRIMARY}`}
                onClick={apply}
              >
                Import
              </button>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
