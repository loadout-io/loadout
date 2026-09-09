import type { ReactElement } from 'react';

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
}: BuildControlsProps): ReactElement {
  const stopping = build?.end === 'running' || build?.end === 'stillRunning';
  const action = stopping
    ? 'Stop'
    : hasVersion
      ? 'Rebuild context'
      : build === null
        ? 'Build context'
        : 'Try building again';

  return (
    <div data-context-build-controls className="card flex flex-col gap-3">
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
        <span className="label">Model (empty means this app's own model)</span>
        <input
          id="context-build-model"
          className="field"
          value={model}
          placeholder={app === 'claude-code' ? 'sonnet' : 'gpt-5.6-sol'}
          onChange={(event) => {
            onModel(event.target.value);
          }}
        />
      </label>

      <div className="flex items-center gap-3">
        <button
          data-build-action
          type="button"
          className="btn-primary"
          onClick={stopping ? onStop : onBuild}
        >
          {action}
        </button>
        {build === null ? null : (
          <span data-build-progress className="value">
            {build.batchesDone} of {build.batchesTotal} batches
          </span>
        )}
      </div>

      {build === null ? null : (
        <>
          <p data-build-said role={build.end === 'failed' ? 'alert' : undefined} className="lead">
            {build.said}
          </p>
          <ul data-source-progress className="flex flex-col gap-1">
            {build.sources.map((source) => (
              <li key={`${source.sourceId}:${source.part}`} className="flex gap-2">
                <span className="text-ink">
                  {sourceNames[source.sourceId] ?? source.sourceId} · {source.part}
                </span>
                <span className="value">{source.outcome}</span>
                {source.said === '' ? null : <span className="lead">{source.said}</span>}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
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
