import type { ReactElement } from 'react';

import type { TriggerCadence } from './io';

export interface TriggerWorkflowOption {
  readonly path: string;
  readonly name: string;
}

export interface TriggerWorkspaceOption {
  readonly id: string;
  readonly name: string;
  readonly folder: string;
}

/** The editable values in the panel. The saved secret is represented by a fact, never a value. */
export interface TriggerFormValue {
  readonly connector: '' | 'linear';
  readonly apiKey: string;
  readonly workflow: string;
  readonly workspace: string;
  readonly pollEveryMinutes: TriggerCadence;
}

export type TriggerConnectionState =
  | { readonly kind: 'idle' }
  | { readonly kind: 'testing' }
  | { readonly kind: 'worked'; readonly sentence: string }
  | { readonly kind: 'refused'; readonly sentence: string };

export interface TriggerFormProps {
  readonly mode: 'create' | 'edit';
  readonly value: TriggerFormValue;
  readonly workflows: readonly TriggerWorkflowOption[];
  readonly workspaces: readonly TriggerWorkspaceOption[];
  readonly hasSavedKey: boolean;
  readonly refusal: string | null;
  readonly connection: TriggerConnectionState;
  readonly confirmingDelete: boolean;
  readonly busy: 'idle' | 'saving' | 'deleting';
  readonly onChange: (value: TriggerFormValue) => void;
  readonly onTest: () => Promise<void>;
  readonly onSave: () => Promise<void>;
  readonly onCancel: () => void;
  readonly onDelete?: () => void;
  readonly onConfirmDelete?: () => Promise<void>;
  readonly onKeep?: () => void;
}

const CADENCES: readonly { readonly value: TriggerCadence; readonly label: string }[] = [
  { value: 1, label: 'Every minute' },
  { value: 5, label: 'Every 5 minutes' },
  { value: 15, label: 'Every 15 minutes' },
  { value: 60, label: 'Every hour' },
];

/* 2026-08-31: siedem stałych z listami klas zeszło do warstwy prymitywów (`theme.css`).
 * `PRIMARY_OFF` znikł bez zamiennika i to jest sedno tej zmiany: stan wyłączony jest REGUŁĄ
 * przy `:disabled`, a nie drugim przyciskiem — a każdy z tych przycisków nosi `disabled` już
 * od dawna, więc bliźniak malował na szaro to, co i tak było wyłączone. */

function cadenceFrom(raw: string, current: TriggerCadence): TriggerCadence {
  const parsed = Number.parseInt(raw, 10);
  return CADENCES.find((choice) => choice.value === parsed)?.value ?? current;
}

function missingForSave(
  mode: TriggerFormProps['mode'],
  value: TriggerFormValue,
  hasSavedKey: boolean,
  workspaces: readonly TriggerWorkspaceOption[],
): string | null {
  if (value.apiKey.trim() === '' && (mode === 'create' || !hasSavedKey)) {
    return 'Enter a Linear API key to save this trigger.';
  }
  if (value.connector !== 'linear') return 'Choose Linear to save this trigger.';
  if (!workspaces.some((workspace) => workspace.folder === value.workspace)) {
    return 'Choose an available workspace to save this trigger.';
  }
  if (value.workflow.trim() === '') return 'Choose a workflow to save this trigger.';
  return null;
}

/** A controlled panel: secrets live only in its caller's draft and are never reconstructed. */
export function TriggerForm({
  mode,
  value,
  workflows,
  workspaces,
  hasSavedKey,
  refusal,
  connection,
  confirmingDelete,
  busy,
  onChange,
  onTest,
  onSave,
  onCancel,
  onDelete,
  onConfirmDelete,
  onKeep,
}: TriggerFormProps): ReactElement {
  const missing = missingForSave(mode, value, hasSavedKey, workspaces);
  const canTest = value.apiKey.trim() !== '' || (mode === 'edit' && hasSavedKey);
  const testing = connection.kind === 'testing';

  return (
    <form
      data-trigger-form
      data-gap="3"
      className="stack"
      onSubmit={(event) => {
        event.preventDefault();
        if (missing !== null || busy !== 'idle') return;
        return onSave();
      }}
    >
      <h2 className="text-heading text-ink">
        {mode === 'create' ? 'New Linear trigger' : 'Edit Linear trigger'}
      </h2>

      <div className="stack">
        <label htmlFor="trigger-connector" className="label">
          Connector
        </label>
        <select
          id="trigger-connector"
          data-trigger-field="connector"
          className="field"
          disabled={busy !== 'idle'}
          value={value.connector}
          onChange={(event) => {
            onChange({
              ...value,
              connector: event.target.value === 'linear' ? 'linear' : value.connector,
            });
          }}
        >
          <option value="" disabled>
            Choose a connector
          </option>
          <option value="linear">Linear</option>
        </select>
      </div>

      <div className="stack">
        <label htmlFor="trigger-api-key" className="label">
          Linear API key
        </label>
        {mode === 'edit' && hasSavedKey ? <p className="lead">A Linear key is saved.</p> : null}
        <input
          id="trigger-api-key"
          data-trigger-field="apiKey"
          className="field"
          type="password"
          autoComplete="new-password"
          disabled={busy !== 'idle'}
          value={value.apiKey}
          aria-label={
            mode === 'edit' && hasSavedKey ? 'Replace the saved Linear API key' : undefined
          }
          onChange={(event) => {
            onChange({ ...value, apiKey: event.target.value });
          }}
        />
        <p className="lead">Create or copy it in Linear Settings → Security &amp; access.</p>
      </div>

      <div className="stack">
        <span className="label">When</span>
        <p data-trigger-condition className="text-ink">
          An issue is assigned to you
        </p>
      </div>

      <div className="stack">
        <label htmlFor="trigger-workspace" className="label">
          Workspace
        </label>
        <select
          id="trigger-workspace"
          data-trigger-field="workspace"
          className="field"
          disabled={busy !== 'idle'}
          value={value.workspace}
          onChange={(event) => {
            onChange({ ...value, workspace: event.target.value });
          }}
        >
          <option value="" disabled>
            Choose a workspace
          </option>
          {value.workspace === '' ||
          workspaces.some((workspace) => workspace.folder === value.workspace) ? null : (
            <option value={value.workspace} disabled>
              Saved workspace is no longer available
            </option>
          )}
          {workspaces.map((workspace) => (
            <option key={workspace.id} value={workspace.folder}>
              {`${workspace.name} — ${workspace.folder}`}
            </option>
          ))}
        </select>
        <p className="lead">
          Runs from this trigger always use this workspace, even while another one is open.
        </p>
      </div>

      <div className="stack">
        <label htmlFor="trigger-cadence" className="label">
          Check every
        </label>
        <select
          id="trigger-cadence"
          data-trigger-field="cadence"
          className="field"
          disabled={busy !== 'idle'}
          value={String(value.pollEveryMinutes)}
          onChange={(event) => {
            onChange({
              ...value,
              pollEveryMinutes: cadenceFrom(event.target.value, value.pollEveryMinutes),
            });
          }}
        >
          {CADENCES.map((choice) => (
            <option key={choice.value} value={String(choice.value)}>
              {choice.label}
            </option>
          ))}
        </select>
        <p data-trigger-cadence-limit className="lead">
          Checks run while Loadout is open.
        </p>
      </div>

      <div className="stack">
        <label htmlFor="trigger-workflow" className="label">
          Workflow
        </label>
        <select
          id="trigger-workflow"
          data-trigger-field="workflow"
          className="field"
          disabled={busy !== 'idle'}
          value={value.workflow}
          onChange={(event) => {
            onChange({ ...value, workflow: event.target.value });
          }}
        >
          {workflows.map((workflow) => (
            <option key={workflow.path} value={workflow.path}>
              {workflow.name}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-wrap items-center gap-2 border-t border-line pt-3">
        <button
          data-trigger-action="test"
          type="button"
          disabled={!canTest || testing || busy !== 'idle'}
          className="btn-quiet"
          onClick={onTest}
        >
          {testing ? 'Testing…' : 'Test connection'}
        </button>
        <button data-trigger-action="cancel" type="button" className="btn-quiet" onClick={onCancel}>
          Cancel
        </button>
        <button
          data-trigger-action="save"
          type="submit"
          disabled={missing !== null || busy !== 'idle'}
          aria-describedby={missing === null ? undefined : 'trigger-save-blocked'}
          className="btn-primary ml-auto"
        >
          {busy === 'saving' ? 'Saving…' : 'Save'}
        </button>
      </div>

      {missing === null ? null : (
        <p id="trigger-save-blocked" data-trigger-save-blocked className="lead">
          {missing}
        </p>
      )}

      {/* WEJŚCIE, bo to zdanie jest CAŁĄ odpowiedzią na „Test connection": bez niego kliknięcie
          kończy się ciszą, a cisza czyta się jak kliknięcie, które nie doszło (DESIGN §7).
          Jedno zdarzenie, jeden region — `refusal` niżej stawia inna droga (Save), więc sufit
          dwóch regionów z ARCHITECTURE §7 zostaje niewyczerpany. */}
      {connection.kind === 'idle' || connection.kind === 'testing' ? null : (
        <p
          data-trigger-connection={connection.kind}
          role={connection.kind === 'refused' ? 'alert' : undefined}
          className="lead enter"
          data-tone={connection.kind === 'refused' ? 'fail' : 'ink'}
        >
          {connection.sentence}
        </p>
      )}

      {refusal === null ? null : (
        <p data-trigger-refusal role="alert" className="lead enter" data-tone="fail">
          {refusal}
        </p>
      )}

      {mode === 'edit' ? (
        <div data-gap="2" className="stack border-t border-line pt-3">
          {confirmingDelete ? (
            <>
              <p data-trigger-delete-warning className="text-ink">
                Any saved issue waiting to start will be discarded.
              </p>
              <div className="flex items-center gap-2">
                <button
                  data-trigger-action="confirm-delete"
                  type="button"
                  disabled={busy !== 'idle'}
                  className="btn-danger"
                  onClick={() => onConfirmDelete?.()}
                >
                  {busy === 'deleting' ? 'Deleting…' : 'Delete trigger'}
                </button>
                <button
                  data-trigger-action="keep"
                  type="button"
                  disabled={busy !== 'idle'}
                  className="btn-quiet"
                  onClick={onKeep}
                >
                  Keep it
                </button>
              </div>
            </>
          ) : (
            <button
              data-trigger-action="delete"
              type="button"
              disabled={busy !== 'idle'}
              className="btn-danger mr-auto"
              onClick={onDelete}
            >
              Delete
            </button>
          )}
        </div>
      ) : null}
    </form>
  );
}
