import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';
import { why } from '../../ipc/why';
import { activeWorkspace } from '../../state/workspaces';
import * as Disk from './io';

export type Compatibility = 'exact' | 'adjusted' | 'needs_choice' | 'unsupported';
export type SourceKind =
  'claude' | 'codex' | 'agent_skills' | 'rulesync' | 'open_standard' | 'unknown';
export type ItemKind =
  'agent' | 'skill' | 'connection' | 'workflow' | 'hook' | 'memory' | 'rule' | 'unknown';

export interface SourceItem {
  id: string;
  source?: SourceKind;
  kind?: ItemKind;
  path: string;
  name: string;
  summary: string;
}

export interface Mapping {
  itemId: string;
  compatibility: Compatibility;
  message: string;
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
  leaveOut: string[];
}

export interface ImportReceipt {
  id: string;
  written: string[];
  enabledConnections: string[];
}

export interface ImportIo {
  scanSetup(workspace: string): Promise<ImportPreview>;
  applySetup(request: ApplyRequest): Promise<ImportReceipt>;
}

export interface ImportSetupProps {
  onClose: () => void;
  onImported: () => void;
  io?: ImportIo;
  initialPreview?: ImportPreview;
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
const WINDOW =
  'flex max-h-full w-full max-w-240 flex-col gap-4 overflow-auto rounded-lg border border-line-strong bg-overlay p-6';
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

const KINDS: Readonly<Record<ItemKind, string>> = {
  agent: 'Agent',
  skill: 'Skill',
  connection: 'Connection',
  workflow: 'Routine',
  hook: 'Hook',
  memory: 'Memory',
  rule: 'Rule',
  unknown: 'Other',
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
  return item.kind === undefined ? KINDS.unknown : KINDS[item.kind];
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
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [saved, setSaved] = useState<ImportReceipt | null>(null);
  const [inventoryView, setInventoryView] = useState<InventoryView>('all');
  const blocked = preview === null ? 0 : blockers(preview, leaveOut);
  const hasItems = preview !== null && preview.snapshot.items.length > 0;
  const unresolved = preview === null ? [] : unresolvedIn(preview);
  /* Ile pozycji naprawdę zostanie poza importem — liczone z listy pominięć, nie z `unresolved`:
   * człowiek mógł którąś odznaczyć i wtedy ona już nie „zostaje poza", tylko blokuje. */
  const leftOut = unresolved.filter((id) => leaveOut.includes(id)).length;
  const mappings = new Map(
    preview?.draft.report.mappings.map((mapping) => [mapping.itemId, mapping]) ?? [],
  );
  const visibleItems =
    preview?.snapshot.items.filter((item) => {
      const compatibility = mappings.get(item.id)?.compatibility ?? 'unsupported';
      const ready = compatibility === 'exact' || compatibility === 'adjusted';
      return inventoryView === 'all' || (inventoryView === 'ready' ? ready : !ready);
    }) ?? [];

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
        leaveOut,
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
              Turn project agents, skills, connections, and routines into Loadout files.
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
          <p className="text-body text-muted">
            Scan reads setup files only. It does not run hooks, skills, agents, or connections.
          </p>
        ) : (
          <>
            <div className="grid grid-cols-4 gap-2 border-y border-line py-3 text-center">
              <span>
                <b className="block text-heading text-ink">{preview.draft.agents.length}</b>
                <small className="text-muted">Agents</small>
              </span>
              <span>
                <b className="block text-heading text-ink">{preview.draft.skills.length}</b>
                <small className="text-muted">Skills</small>
              </span>
              <span>
                <b className="block text-heading text-ink">{preview.draft.connections.length}</b>
                <small className="text-muted">Connections</small>
              </span>
              <span>
                <b className="block text-heading text-ink">{preview.draft.workflows.length}</b>
                <small className="text-muted">Workflows</small>
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
            </div>
            <div className="min-h-0 overflow-auto rounded-md border border-line">
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
                  {visibleItems.map((item) => {
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
            <div className="flex items-center gap-3 border-t border-line pt-3">
              {unresolved.length === 0 ? null : (
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
                    ? `${String(blocked)} item(s) need a choice. Tick Skip to leave them out.`
                    : leftOut === 0
                      ? 'Ready to import.'
                      : `Ready to import. ${String(leftOut)} item(s) will be left out.`}
              </p>
              <button
                type="button"
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
