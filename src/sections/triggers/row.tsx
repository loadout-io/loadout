import type { ReactElement } from 'react';

import type { TriggerVisibleStatus, TriggerView } from '../../state/triggers';

export interface TriggerRowProps {
  readonly trigger: TriggerView;
  readonly onToggle: (slug: string, enabled: boolean) => Promise<void>;
}

interface SaidStatus {
  readonly sentence: string;
  readonly receipt?: string;
}

function utcReceipt(milliseconds: number): { readonly label: string; readonly iso: string } {
  const receipt = new Date(milliseconds);
  if (Number.isNaN(receipt.getTime())) {
    return { label: 'an unknown receipt time', iso: '' };
  }
  const iso = receipt.toISOString();
  return { label: `${iso.slice(0, 19).replace('T', ' ')} UTC`, iso };
}

function says(status: TriggerVisibleStatus): SaidStatus {
  if (status.kind === 'unchecked') return { sentence: 'Not checked yet.' };
  if (status.kind === 'armed') {
    return { sentence: 'Watching for new issues. Nothing has started yet.' };
  }
  if (status.kind === 'busy') {
    return {
      sentence: `${status.delivery.issue.identifier} is saved and will start when the current run finishes.`,
    };
  }
  if (status.kind === 'refused') return { sentence: status.sentence };

  const receipt = utcReceipt(status.receiptAt);
  return {
    sentence: `Started ${status.workflow} at ${receipt.label}.`,
    receipt: receipt.iso,
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

/** One library row: at most four visible facts and one switch, with no invented broken config. */
export function TriggerRow({ trigger, onToggle }: TriggerRowProps): ReactElement {
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
      <div className="grid min-w-0 grid-cols-3 items-center gap-3">
        <span data-trigger-text className="text-label text-ink">
          {sourceName(trigger.source)}
        </span>
        <span data-trigger-text className="truncate text-body text-ink">
          {conditionName(trigger.condition)}
        </span>
        <span data-trigger-text className="truncate text-body text-muted">
          {workflow}
        </span>
      </div>
      <div className="flex items-center gap-3">
        <span
          data-trigger-text
          data-trigger-status
          className="max-w-96 text-right text-label text-muted"
        >
          {status.sentence}
        </span>
        {status.receipt === undefined ? null : <time aria-hidden dateTime={status.receipt} />}
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
