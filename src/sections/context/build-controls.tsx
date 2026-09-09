import type { ReactElement, ReactNode } from 'react';

import type { AgentAppStatus } from '../../state/agent-apps';
import type { ContextApp, ContextBuild } from '../../state/context';

export interface BuildControlsProps {
  build: ContextBuild | null;
  app: ContextApp;
  model: string;
  claudeCode: AgentAppStatus;
  codex: AgentAppStatus;
  hasVersion: boolean;
  sourceNames: Readonly<Record<string, string>>;
  onChooseApp: (app: ContextApp) => void;
  onModel: (model: string) => void;
  onBuild: () => void;
  onStop: () => void;
  options?: ReactNode;
  secondaryAction?: ReactNode;
  disabled?: boolean;
  pending?: string | null;
  preparing?: string | null;
}

export default function BuildControls({
  build,
  app,
  model,
  claudeCode,
  codex,
  hasVersion,
  sourceNames,
  onChooseApp,
  onModel,
  onBuild,
  onStop,
  options,
  secondaryAction,
  disabled = false,
  pending = null,
  preparing = null,
}: BuildControlsProps): ReactElement {
  const stopping = preparing !== null || build?.end === 'running' || build?.end === 'stillRunning';
  const action = stopping
    ? 'Stop'
    : hasVersion
      ? 'Rebuild context'
      : build === null
        ? 'Build context'
        : 'Try building again';

  return (
    <div data-context-build-controls className="flex flex-col gap-3">
      <details data-context-options>
        <summary className="caption cursor-pointer">Options</summary>
        <fieldset disabled={pending !== null || stopping} className="mt-3 flex flex-col gap-3">
          {options}
          <div className="flex flex-wrap gap-3">
            <AppChoice
              name="Claude Code"
              value="claude-code"
              selected={app === 'claude-code'}
              status={claudeCode}
              onChoose={onChooseApp}
            />
            <AppChoice
              name="Codex"
              value="codex"
              selected={app === 'codex'}
              status={codex}
              onChoose={onChooseApp}
            />
          </div>

          <label className="flex flex-col gap-1" htmlFor="context-build-model">
            <span className="label">Model (optional)</span>
            <input
              id="context-build-model"
              className="field"
              value={model}
              placeholder="Default model"
              onChange={(event) => {
                onModel(event.target.value);
              }}
            />
            <span className="caption">Leave blank to use this app's default model.</span>
          </label>
        </fieldset>
      </details>

      <div className="flex items-center gap-3">
        <button
          data-build-action
          type="button"
          className="btn-primary"
          disabled={!stopping && disabled}
          onClick={stopping ? onStop : onBuild}
        >
          {pending !== null && !stopping ? pending : action}
        </button>
        {secondaryAction}
        <span className="caption ml-auto">
          Using {app === 'claude-code' ? 'Claude Code' : 'Codex'} ·{' '}
          {model.trim() || 'Default model'}
        </span>
      </div>

      {preparing !== null ? (
        <p role="status" className="lead">
          Preparing your document…
        </p>
      ) : build === null || pending !== null ? null : (
        <>
          {build.end === 'ready' ? null : (
            <p data-build-said role={build.end === 'failed' ? 'alert' : undefined} className="lead">
              {build.end === 'running' ? progressSaid(build) : build.said}
            </p>
          )}
          <details data-build-details>
            <summary className="caption cursor-pointer">Details</summary>
            <div className="mt-2 flex flex-col gap-2">
              <span data-build-progress className="value">
                {build.batchesDone} of {build.batchesTotal} batches
              </span>
              {build.end === 'running' ? <p className="lead">{build.said}</p> : null}
              <ul data-source-progress className="flex flex-col gap-1">
                {build.sources.map((source) => (
                  <li key={`${source.sourceId}:${source.part}`} className="flex gap-2">
                    <span className="text-ink">
                      {sourceNames[source.sourceId] ?? source.sourceId} · {source.part}
                    </span>
                    <span className="value">
                      {
                        {
                          processed: 'Read',
                          excluded: 'Left out',
                          failed: 'Needs attention',
                          unknown: 'Waiting',
                        }[source.outcome]
                      }
                    </span>
                    {source.said === '' ? null : <span className="lead">{source.said}</span>}
                  </li>
                ))}
              </ul>
            </div>
          </details>
        </>
      )}
    </div>
  );
}

function progressSaid(build: ContextBuild): string {
  if (build.stage === 'grouping') return 'Organizing what matters…';
  if (build.stage === 'publishing') return 'Saving your context…';
  if (build.stage === 'extracting') return 'Reading your material…';
  return 'Preparing your material…';
}

function AppChoice({
  name,
  value,
  selected,
  status,
  onChoose,
}: {
  readonly name: string;
  readonly value: ContextApp;
  readonly selected: boolean;
  readonly status: AgentAppStatus;
  readonly onChoose: (app: ContextApp) => void;
}): ReactElement {
  return (
    <label className="card flex min-w-48 flex-1 items-start gap-2">
      <input
        type="radio"
        name="context-build-app"
        value={value}
        checked={selected}
        disabled={status.state !== 'found'}
        onChange={() => {
          onChoose(value);
        }}
      />
      <span className="flex flex-col gap-1">
        <span className="text-heading text-ink">{name}</span>
        <span className="lead">{statusSentence(name, status)}</span>
      </span>
    </label>
  );
}

function statusSentence(name: string, status: AgentAppStatus): string {
  if (status.state === 'found')
    return `${name} ${status.version} is installed. Sign-in is checked when the build starts.`;
  if (status.state === 'not-found') return `${name} is not installed.`;
  if (status.state === 'could-not-check') return `Loadout could not check ${name}.`;
  return `Loadout is checking ${name}.`;
}
