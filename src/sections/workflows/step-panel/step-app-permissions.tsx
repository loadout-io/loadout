import type { ReactElement } from 'react';
import type { ServiceGrant, ServiceOperation } from '../../../state/agents';

const LABELS: Record<ServiceOperation, string> = {
  read: 'Read',
  start: 'Start',
  restart: 'Restart',
  stop: 'Stop',
};

export function StepAppPermissions({
  ceiling,
  value,
  onChange,
}: {
  ceiling: ServiceGrant[];
  value: ServiceGrant[];
  onChange: (next: ServiceGrant[]) => void;
}): ReactElement {
  return (
    <section className="stack" aria-label="Apps this step may use">
      <p className="lead">Apps this step may use</p>
      {ceiling.length === 0 ? (
        <p className="lead">This agent has no app permissions. Change the agent first.</p>
      ) : null}
      {ceiling.map((grant) => (
        <div className="stack" key={grant.service}>
          <p>{grant.service}</p>
          <div className="flex flex-wrap gap-3">
            {grant.operations.map((operation) => (
              <label className="flex items-center gap-1" key={operation}>
                <input
                  type="checkbox"
                  aria-label={`${LABELS[operation]} ${grant.service}`}
                  checked={value.some(
                    (current) =>
                      current.service === grant.service && current.operations.includes(operation),
                  )}
                  onChange={(event) =>
                    onChange(
                      ceiling
                        .map((allowed) => ({
                          service: allowed.service,
                          operations: allowed.operations.filter((choice) =>
                            allowed.service === grant.service && choice === operation
                              ? event.target.checked
                              : value.some(
                                  (current) =>
                                    current.service === allowed.service &&
                                    current.operations.includes(choice),
                                ),
                          ),
                        }))
                        .filter((current) => current.operations.length > 0),
                    )
                  }
                />
                {LABELS[operation]}
              </label>
            ))}
          </div>
        </div>
      ))}
      <p className="lead">This step can use fewer of its agent's permissions, never more.</p>
    </section>
  );
}
