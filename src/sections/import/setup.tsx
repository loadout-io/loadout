import type { FormEvent, ReactElement } from 'react';
import { useEffect, useState } from 'react';
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

export interface ImportedConnection {
  id: string;
  name: string;
  enabled: boolean;
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
  analysis?: {
    vendor: AnalysisVendor;
    sourceHashes: Record<string, string>;
    agents: unknown[];
    workflows: unknown[];
  };
}

export type AnalysisVendor = 'claude-code' | 'codex';

export interface AnalysisRequest {
  workspace: string;
  vendor: AnalysisVendor;
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
  analyzeSetup?(request: AnalysisRequest): Promise<ImportPreview | null>;
  stopSetupAnalysis?(): Promise<void>;
}

export interface ImportSetupProps {
  onClose: () => void;
  onImported: () => void;
  io?: ImportIo;
  initialPreview?: ImportPreview;
}

const BACKDROP = 'fixed inset-0 z-20 flex items-center justify-center bg-bg/72 p-6';
const WINDOW =
  'flex max-h-full w-full max-w-240 flex-col gap-4 overflow-auto rounded-lg border border-line-strong bg-panel p-6';
const BUTTON = 'h-8 rounded-sm border border-line px-3 text-ui text-body';
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
type Busy = 'scan' | 'analyze' | 'apply' | null;

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
  const [leaveOut, setLeaveOut] = useState<string[]>([]);
  const [busy, setBusy] = useState<Busy>(null);
  const [analysisVendor, setAnalysisVendor] = useState<AnalysisVendor>('claude-code');
  const [analysisSeconds, setAnalysisSeconds] = useState(0);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [saved, setSaved] = useState<ImportReceipt | null>(null);
  const [inventoryView, setInventoryView] = useState<InventoryView>('all');
  const blocked = preview === null ? 0 : blockers(preview, leaveOut);
  const hasItems = preview !== null && preview.snapshot.items.length > 0;
  const unresolved =
    preview?.draft.report.mappings
      .filter(
        (mapping) =>
          mapping.compatibility === 'unsupported' || mapping.compatibility === 'needs_choice',
      )
      .map((mapping) => mapping.itemId) ?? [];
  const mappings = new Map(
    preview?.draft.report.mappings.map((mapping) => [mapping.itemId, mapping]) ?? [],
  );
  const visibleItems =
    preview?.snapshot.items.filter((item) => {
      const compatibility = mappings.get(item.id)?.compatibility ?? 'unsupported';
      const ready = compatibility === 'exact' || compatibility === 'adjusted';
      return inventoryView === 'all' || (inventoryView === 'ready' ? ready : !ready);
    }) ?? [];

  useEffect(() => {
    if (busy !== 'analyze') return undefined;
    const timer = window.setInterval(() => {
      setAnalysisSeconds((seconds) => seconds + 1);
    }, 1_000);
    return () => {
      window.clearInterval(timer);
    };
  }, [busy]);

  const scan = (event: FormEvent): void => {
    event.preventDefault();
    if (workspace.trim() === '') return;
    setBusy('scan');
    setRefusal(null);
    setSaved(null);
    void io
      .scanSetup(workspace.trim())
      .then((next) => {
        setPreview(next);
        setEnabled([]);
        setLeaveOut([]);
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not inspect that folder.'));
      })
      .finally(() => {
        setBusy(null);
      });
  };

  const analyze = (): void => {
    if (preview === null || io.analyzeSetup === undefined) return;
    setBusy('analyze');
    setAnalysisSeconds(0);
    setRefusal(null);
    setSaved(null);
    void io
      .analyzeSetup({ workspace: preview.snapshot.root, vendor: analysisVendor })
      .then((next) => {
        if (next !== null) {
          setPreview(next);
          setLeaveOut([]);
        }
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not analyze that setup.'));
      })
      .finally(() => {
        setBusy(null);
      });
  };

  const apply = (): void => {
    if (preview === null || !hasItems || blocked > 0) return;
    setBusy('apply');
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
        setBusy(null);
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
          <button
            type="submit"
            disabled={busy !== null || workspace.trim() === ''}
            className={PRIMARY}
          >
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
            {preview.analysis === undefined ? null : (
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
            )}
            {preview.analysis === undefined ? null : (
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
            )}
            {preview.analysis !== undefined && blocked === 0 ? null : (
              <section className="rounded-md border border-line bg-well p-3">
                <div className="flex flex-wrap items-end gap-2">
                  <label className="flex flex-col gap-1 text-label text-muted">
                    Analyze remaining setup with
                    <select
                      value={analysisVendor}
                      disabled={busy !== null}
                      onChange={(event) => {
                        setAnalysisVendor(event.target.value as AnalysisVendor);
                      }}
                      className="h-8 rounded-sm border border-line bg-panel px-3 text-body text-ink"
                    >
                      <option value="claude-code">Claude · Sonnet · high effort</option>
                      <option value="codex">Codex</option>
                    </select>
                  </label>
                  {busy === 'analyze' ? (
                    <button
                      type="button"
                      className={BUTTON}
                      onClick={() => {
                        void io.stopSetupAnalysis?.();
                      }}
                    >
                      Stop analysis
                    </button>
                  ) : (
                    <button
                      type="button"
                      className={PRIMARY}
                      disabled={io.analyzeSetup === undefined || busy !== null}
                      onClick={analyze}
                    >
                      Analyze and convert
                    </button>
                  )}
                  <p className="min-w-64 flex-1 text-note text-muted">
                    Sends a redacted, read-only copy of setup files to the selected service. The
                    result is checked before it can be imported.
                  </p>
                </div>
                {busy === 'analyze' ? (
                  <p role="status" className="mt-3 text-body text-ink">
                    {`Analyzing setup in parallel… ${String(analysisSeconds)}s`}
                  </p>
                ) : preview.analysis === undefined ? (
                  <p className="mt-3 text-note text-muted">
                    Raw scan results stay hidden until the analysis is ready to review.
                  </p>
                ) : null}
              </section>
            )}
            {preview.analysis === undefined ? null : (
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
            )}
            {preview.analysis === undefined || preview.draft.connections.length === 0 ? null : (
              <fieldset className="flex flex-col gap-2 border-t border-line pt-3">
                <legend className="text-subhead text-ink">
                  Connections stay off unless you enable them
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
                  </label>
                ))}
              </fieldset>
            )}
            {preview.analysis === undefined ? null : (
              <section className="rounded-md border border-line bg-well p-3">
                <h3 className="text-subhead text-ink">Review analyzed routine</h3>
                <p className="text-note text-muted">
                  {`Analyzed with ${preview.analysis.vendor === 'codex' ? 'Codex' : 'Claude'}`}
                </p>
                {preview.draft.workflows.length === 0 ? (
                  <p className="text-note text-muted">No complete routine was added.</p>
                ) : (
                  <ul className="mt-2 grid gap-2">
                    {preview.draft.workflows.map((workflow) => (
                      <li key={workflow.id} className="rounded-sm border border-line bg-panel p-2">
                        <b className="text-body text-ink">{workflow.name}</b>
                        <span className="ml-2 text-note text-muted">
                          {`${String(workflow.steps?.length ?? 0)} steps`}
                        </span>
                        <ul className="mt-1 flex flex-wrap gap-1">
                          {workflow.steps?.map((step) => (
                            <li
                              key={step.id}
                              className="rounded-pill border border-line px-2 py-0.5 text-note text-body"
                            >
                              {step.command === undefined
                                ? step.name
                                : `${step.name}: ${step.command}`}
                            </li>
                          ))}
                        </ul>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            )}
            {preview.analysis === undefined ? null : (
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
                    : blocked === 0
                      ? 'Ready to import.'
                      : `${String(blocked)} item(s) must be resolved before import.`}
                </p>
                <button
                  type="button"
                  disabled={busy !== null || !hasItems || blocked > 0}
                  className={`ml-auto ${PRIMARY}`}
                  onClick={apply}
                >
                  Import
                </button>
              </div>
            )}
          </>
        )}
      </section>
    </div>
  );
}
