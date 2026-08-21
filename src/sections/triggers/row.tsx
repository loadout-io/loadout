import type { ReactElement } from 'react';

import type { TriggerVisibleStatus, TriggerView } from '../../state/triggers';

export interface TriggerRowProps {
  readonly trigger: TriggerView;
  readonly onToggle: (slug: string, enabled: boolean) => Promise<void>;
  readonly onOpen: (slug: string) => void;
}

interface SaidStatus {
  readonly sentence: string;
  readonly machineTime?: string;
}

function utcStartTime(milliseconds: number): { readonly label: string; readonly iso: string } {
  const started = new Date(milliseconds);
  if (Number.isNaN(started.getTime())) {
    return { label: 'an unknown start time', iso: '' };
  }
  const iso = started.toISOString();
  return { label: `${iso.slice(0, 19).replace('T', ' ')} UTC`, iso };
}

function says(status: TriggerVisibleStatus): SaidStatus {
  if (status.kind === 'unchecked') return { sentence: 'Not checked yet.' };
  if (status.kind === 'armed') {
    return { sentence: 'Watching for new issues. Nothing has started yet.' };
  }
  if (status.kind === 'busy') {
    return {
      sentence: `${status.delivery.issue.identifier} is saved while Loadout handles the run.`,
    };
  }
  if (status.kind === 'refused') return { sentence: status.sentence };

  const started = utcStartTime(status.receiptAt);
  return {
    sentence: `Started ${status.workflow} at ${started.label}.`,
    machineTime: started.iso,
  };
}

function sourceName(source: string): string {
  return source.toLowerCase() === 'linear' ? 'Linear' : 'Issue tracker';
}

function conditionName(condition: string): string {
  const words = condition.trim().replace(/[-_]+/g, ' ').replace(/\s+/g, ' ');
  if (words.toLowerCase() === 'assigned to me') return 'Assigned to you';
  return words.length === 0 ? 'No condition saved' : words[0]?.toUpperCase() + words.slice(1);
}

function cadenceName(minutes: number): string {
  if (minutes === 1) return 'Every minute';
  if (minutes === 60) return 'Every hour';
  return `Every ${String(minutes)} minutes`;
}

/** One library row: at most four visible facts and one switch, with no invented broken config. */
export function TriggerRow({ trigger, onToggle, onOpen }: TriggerRowProps): ReactElement {
  if (trigger.problem !== undefined) {
    return (
      <li
        data-trigger-row={trigger.slug}
        className="flex flex-col gap-1 border-b border-line px-4 py-3 last:border-b-0"
      >
        <span data-trigger-text className="font-mono text-mono text-muted">
          {trigger.slug}
        </span>
        <p data-trigger-text data-trigger-status className="text-body text-attend">
          {trigger.problem}
        </p>
      </li>
    );
  }

  const status = says(trigger.status);
  const workflow = trigger.workflowName ?? `Workflow ${trigger.workflow} is missing.`;
  return (
    <li
      data-trigger-row={trigger.slug}
      className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-line px-4 py-3 last:border-b-0"
    >
      <button
        data-trigger-open
        type="button"
        aria-label={`Edit ${trigger.slug}`}
        className="grid min-w-0 grid-cols-3 items-center gap-3 text-left"
        onClick={() => {
          onOpen(trigger.slug);
        }}
      >
        <span data-trigger-text className="text-label text-ink">
          {sourceName(trigger.source)}
        </span>
        <span data-trigger-text className="truncate text-body text-ink">
          {conditionName(trigger.condition)}
        </span>
        <span data-trigger-text className="truncate text-body text-muted">
          {`${workflow} · ${cadenceName(trigger.pollEveryMinutes)}`}
        </span>
      </button>
      <div className="flex items-center gap-3">
        <span
          data-trigger-text
          data-trigger-status
          className="max-w-96 text-right text-label text-muted"
        >
          {status.sentence}
        </span>
        {status.machineTime === undefined ? null : (
          <time aria-hidden dateTime={status.machineTime} />
        )}
        <button
          type="button"
          data-trigger-toggle
          aria-pressed={trigger.enabled}
          aria-label={`${trigger.enabled ? 'Turn off' : 'Turn on'} ${trigger.slug}`}
          className="flex h-5 w-9 items-center rounded-pill border border-line-strong bg-raised px-0.5"
          onClick={() => {
            void onToggle(trigger.slug, !trigger.enabled);
          }}
        >
          <span
            aria-hidden
            className={`size-3.5 rounded-pill ${trigger.enabled ? 'ml-auto bg-accent' : 'bg-line-strong'}`}
          />
        </button>
      </div>
    </li>
  );
}
