import type { ReactElement } from 'react';

import type { TriggerCadence } from './io';

export interface TriggerWorkflowOption {
  readonly path: string;
  readonly name: string;
}

/** The editable values in the panel. The saved secret is represented by a fact, never a value. */
export interface TriggerFormValue {
  readonly connector: '' | 'linear';
  readonly apiKey: string;
  readonly workflow: string;
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

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const FIELD = 'field';
const PRIMARY = 'h-8 rounded-sm bg-accent px-4 text-ui text-bg';
const PRIMARY_OFF = 'h-8 rounded-sm bg-raised px-4 text-ui text-muted';
const QUIET = 'h-8 rounded-sm border border-line px-3 text-ui text-body';
const DANGER = 'h-8 rounded-sm border border-fail-edge px-3 text-ui text-fail';

function cadenceFrom(raw: string, current: TriggerCadence): TriggerCadence {
  const parsed = Number.parseInt(raw, 10);
  return CADENCES.find((choice) => choice.value === parsed)?.value ?? current;
}

function missingForSave(
  mode: TriggerFormProps['mode'],
  value: TriggerFormValue,
  hasSavedKey: boolean,
): string | null {
  if (value.apiKey.trim() === '' && (mode === 'create' || !hasSavedKey)) {
    return 'Enter a Linear API key to save this trigger.';
  }
  if (value.connector !== 'linear') return 'Choose Linear to save this trigger.';
  if (value.workflow.trim() === '') return 'Choose a workflow to save this trigger.';
  return null;
}

/** A controlled panel: secrets live only in its caller's draft and are never reconstructed. */
export function TriggerForm({
  mode,
  value,
  workflows,
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
  const missing = missingForSave(mode, value, hasSavedKey);
  const canTest = value.apiKey.trim() !== '' || (mode === 'edit' && hasSavedKey);
  const testing = connection.kind === 'testing';

  return (
    <form
      data-trigger-form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        if (missing !== null || busy !== 'idle') return;
        return onSave();
      }}
    >
      <h2 className="text-heading text-ink">
        {mode === 'create' ? 'New Linear trigger' : 'Edit Linear trigger'}
      </h2>

      <div className={ROW}>
        <label htmlFor="trigger-connector" className={LABEL}>
          Connector
        </label>
        <select
          id="trigger-connector"
          data-trigger-field="connector"
          className={FIELD}
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

      <div className={ROW}>
        <label htmlFor="trigger-api-key" className={LABEL}>
          Linear API key
        </label>
        {mode === 'edit' && hasSavedKey ? (
          <p className="text-body text-muted">A Linear key is saved.</p>
        ) : null}
        <input
          id="trigger-api-key"
          data-trigger-field="apiKey"
          className={FIELD}
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
        <p className="text-note text-muted">
          Create or copy it in Linear Settings → Security &amp; access.
        </p>
      </div>

      <div className={ROW}>
        <span className={LABEL}>When</span>
        <p data-trigger-condition className="text-body text-ink">
          An issue is assigned to you
        </p>
      </div>

      <div className={ROW}>
        <label htmlFor="trigger-cadence" className={LABEL}>
          Check every
        </label>
        <select
          id="trigger-cadence"
          data-trigger-field="cadence"
          className={FIELD}
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
        <p data-trigger-cadence-limit className="text-note text-muted">
          Checks run while Loadout is open.
        </p>
      </div>

      <div className={ROW}>
        <label htmlFor="trigger-workflow" className={LABEL}>
          Workflow
        </label>
        <select
          id="trigger-workflow"
          data-trigger-field="workflow"
          className={FIELD}
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
          className={!canTest || testing || busy !== 'idle' ? PRIMARY_OFF : QUIET}
          onClick={onTest}
        >
          {testing ? 'Testing…' : 'Test connection'}
        </button>
        <button data-trigger-action="cancel" type="button" className={QUIET} onClick={onCancel}>
          Cancel
        </button>
        <button
          data-trigger-action="save"
          type="submit"
          disabled={missing !== null || busy !== 'idle'}
          aria-describedby={missing === null ? undefined : 'trigger-save-blocked'}
          className={`ml-auto ${missing === null && busy === 'idle' ? PRIMARY : PRIMARY_OFF}`}
        >
          {busy === 'saving' ? 'Saving…' : 'Save'}
        </button>
      </div>

      {missing === null ? null : (
        <p id="trigger-save-blocked" data-trigger-save-blocked className="text-body text-muted">
          {missing}
        </p>
      )}

      {connection.kind === 'idle' || connection.kind === 'testing' ? null : (
        <p
          data-trigger-connection={connection.kind}
          role={connection.kind === 'refused' ? 'alert' : undefined}
          className={connection.kind === 'refused' ? 'text-body text-fail' : 'text-body text-ink'}
        >
          {connection.sentence}
        </p>
      )}

      {refusal === null ? null : (
        <p data-trigger-refusal role="alert" className="text-body text-fail">
          {refusal}
        </p>
      )}

      {mode === 'edit' ? (
        <div className="flex flex-col gap-2 border-t border-line pt-3">
          {confirmingDelete ? (
            <>
              <p data-trigger-delete-warning className="text-body text-ink">
                Any saved issue waiting to start will be discarded.
              </p>
              <div className="flex items-center gap-2">
                <button
                  data-trigger-action="confirm-delete"
                  type="button"
                  disabled={busy !== 'idle'}
                  className={DANGER}
                  onClick={() => onConfirmDelete?.()}
                >
                  {busy === 'deleting' ? 'Deleting…' : 'Delete trigger'}
                </button>
                <button
                  data-trigger-action="keep"
                  type="button"
                  disabled={busy !== 'idle'}
                  className={QUIET}
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
              className={`mr-auto ${DANGER}`}
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
