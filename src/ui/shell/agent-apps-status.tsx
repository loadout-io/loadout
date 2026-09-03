import type { ReactElement } from 'react';
import { useSyncExternalStore } from 'react';

import { useAgentApps } from '../../state/agent-apps';
import type { AgentAppStatus } from '../../state/agent-apps';

function sentence(label: 'Claude Code' | 'Codex', status: AgentAppStatus): string {
  switch (status.state) {
    case 'checking':
      return `Checking ${label}…`;
    case 'found':
      return `${label} · ${status.version}`;
    case 'not-found':
      return `${label} wasn't found.`;
    case 'could-not-check':
      return `Loadout couldn't check ${label}.`;
  }
}

function StatusLine({ children }: { readonly children: string }): ReactElement {
  return (
    <div className="flex items-start gap-[7px]">
      <span aria-hidden className="mt-[3px] size-[7px] shrink-0 rounded-full bg-muted" />
      <span>{children}</span>
    </div>
  );
}

/** Jedyne żywe miejsce, w którym okno pokazuje migawkę lokalnych aplikacji agentów. */
export function AgentAppsStatus(): ReactElement {
  /* Bieżący stan jest też migawką serwerową: test `renderToStaticMarkup` ma czytać ten sam
   * globalny magazyn co prawdziwa stopka, a nie początkowe wartości zustanda. */
  const apps = useSyncExternalStore(
    useAgentApps.subscribe,
    useAgentApps.getState,
    useAgentApps.getState,
  );
  const canRetry = [apps.claudeCode, apps.codex].some(
    (status) => status.state === 'not-found' || status.state === 'could-not-check',
  );

  return (
    <div data-agent-apps-status className="flex flex-col gap-[6px] font-mono text-meta text-muted">
      <StatusLine>{sentence('Claude Code', apps.claudeCode)}</StatusLine>
      <StatusLine>{sentence('Codex', apps.codex)}</StatusLine>
      <span>Sign-in is checked when you first run an agent.</span>
      {canRetry ? (
        <button
          type="button"
          onClick={() => {
            void useAgentApps.getState().check();
          }}
          className="w-fit text-accent hover:text-ink"
        >
          Retry
        </button>
      ) : null}
    </div>
  );
}
