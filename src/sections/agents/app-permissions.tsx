/* WF-28: wybór praw jest osobny od Internetu i zapisu plików. */
import type { ReactElement } from 'react';
import type { Agent, ServiceGrant, ServiceOperation } from '../../state/agents';

const OPERATIONS: readonly { value: ServiceOperation; label: string }[] = [
  { value: 'read', label: 'Read' },
  { value: 'start', label: 'Start' },
  { value: 'restart', label: 'Restart' },
  { value: 'stop', label: 'Stop' },
];

export function AppPermissions({
  value,
  onChange,
}: {
  value: Agent;
  onChange: (next: Agent) => void;
}): ReactElement {
  const grants = value.serviceAccess ?? [];
  const change = (serviceAccess: ServiceGrant[]): void => onChange({ ...value, serviceAccess });
  const edit = (index: number, grant: ServiceGrant): void => {
    change(grants.map((current, at) => (at === index ? grant : current)));
  };
  return (
    <section className="stack" aria-label="Apps this agent may use">
      <p className="lead">Apps this agent may use</p>
      {grants.length === 0 ? <p className="lead">No apps allowed.</p> : null}
      {grants.map((grant, index) => (
        <div className="stack" key={index}>
          <label htmlFor={`agent-app-${index}`} className="label">
            App step {index + 1}
          </label>
          <input
            id={`agent-app-${index}`}
            className="field"
            value={grant.service}
            placeholder="s_preview"
            onChange={(event) => edit(index, { ...grant, service: event.target.value })}
          />
          <div className="flex flex-wrap gap-3">
            {OPERATIONS.map((operation) => (
              <label key={operation.value} className="flex items-center gap-1">
                <input
                  type="checkbox"
                  aria-label={`${operation.label} app ${index + 1}`}
                  checked={grant.operations.includes(operation.value)}
                  onChange={(event) =>
                    edit(index, {
                      ...grant,
                      operations: OPERATIONS.filter((choice) =>
                        choice.value === operation.value
                          ? event.target.checked
                          : grant.operations.includes(choice.value),
                      ).map((choice) => choice.value),
                    })
                  }
                />
                {operation.label}
              </label>
            ))}
          </div>
          <button
            type="button"
            className="button"
            onClick={() => change(grants.filter((_, at) => at !== index))}
          >
            Remove app {index + 1}
          </button>
        </div>
      ))}
      <button
        type="button"
        className="button"
        onClick={() => change([...grants, { service: '', operations: [] }])}
      >
        Allow an app
      </button>
      <p className="lead">
        Use the saved app step ID, including its copy and try when needed. Read means status,
        addresses and limited logs. A workflow step may only reduce these permissions. Internet and
        file permissions stay unchanged.
      </p>
    </section>
  );
}
