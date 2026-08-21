import type { FormEvent, ReactElement } from 'react';
import { useState } from 'react';
import { why } from '../../ipc/why';
import * as Disk from './io';

export type Compatibility = 'exact' | 'adjusted' | 'needs_choice' | 'unsupported';

export interface SourceItem {
  id: string;
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
    workflows: Array<{ id: string; name: string }>;
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

const BACKDROP = 'fixed inset-0 z-20 flex items-center justify-center bg-bg/72 p-6';
const WINDOW =
  'flex max-h-full w-full max-w-180 flex-col gap-4 overflow-auto rounded-lg border border-line-strong bg-panel p-6';
const BUTTON = 'h-8 rounded-sm border border-line px-3 text-ui text-body';
const PRIMARY = 'h-8 rounded-sm bg-accent px-3 text-ui text-bg';

const STATUS: Readonly<Record<Compatibility, string>> = {
  exact: 'Exact',
  adjusted: 'Adjusted',
  needs_choice: 'Needs a choice',
  unsupported: "Can't be reproduced",
};

function blockers(preview: ImportPreview, leaveOut: readonly string[]): number {
  return preview.draft.report.mappings.filter(
    (mapping) =>
      mapping.compatibility === 'unsupported' ||
      (mapping.compatibility === 'needs_choice' && !leaveOut.includes(mapping.itemId)),
  ).length;
}

export function ImportSetup({
  onClose,
  onImported,
  io = Disk,
  initialPreview,
}: ImportSetupProps): ReactElement {
  const [workspace, setWorkspace] = useState(initialPreview?.snapshot.root ?? '');
  const [preview, setPreview] = useState<ImportPreview | null>(initialPreview ?? null);
  const [enabled, setEnabled] = useState<string[]>([]);
  const [leaveOut, setLeaveOut] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [saved, setSaved] = useState<ImportReceipt | null>(null);
  const blocked = preview === null ? 0 : blockers(preview, leaveOut);

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
        setLeaveOut([]);
      })
      .catch((error: unknown) => {
        setRefusal(why(error, 'Loadout could not inspect that folder.'));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  const apply = (): void => {
    if (preview === null || blocked > 0) return;
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
            <ul className="flex flex-col gap-2">
              {preview.snapshot.items.map((item) => {
                const mapping = preview.draft.report.mappings.find((one) => one.itemId === item.id);
                return (
                  <li key={item.id} className="rounded-md border border-line bg-raised p-3">
                    <div className="flex items-center gap-2">
                      <b className="text-body text-ink">{item.name}</b>
                      <span className="ml-auto text-label text-muted">
                        {mapping === undefined
                          ? "Can't be reproduced"
                          : STATUS[mapping.compatibility]}
                      </span>
                    </div>
                    <p className="font-mono text-meta text-muted">{item.path}</p>
                    <p className="text-note text-body">{mapping?.message ?? item.summary}</p>
                    {mapping?.compatibility === 'needs_choice' ? (
                      <label className="mt-2 flex items-center gap-2 text-body text-ink">
                        <input
                          type="checkbox"
                          checked={leaveOut.includes(item.id)}
                          onChange={(event) => {
                            setLeaveOut((now) =>
                              event.target.checked
                                ? [...now, item.id]
                                : now.filter((id) => id !== item.id),
                            );
                          }}
                        />
                        Leave this behavior out of the imported setup
                      </label>
                    ) : null}
                  </li>
                );
              })}
            </ul>
            {preview.draft.connections.length === 0 ? null : (
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
            <div className="flex items-center gap-3 border-t border-line pt-3">
              <p className="text-note text-muted">
                {blocked === 0
                  ? 'Ready to import.'
                  : `${String(blocked)} item(s) must be resolved before import.`}
              </p>
              <button
                type="button"
                disabled={busy || blocked > 0}
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
