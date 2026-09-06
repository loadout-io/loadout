/* WF-13: pole wiąże wyłącznie źródło zwrócone przez wspólny resolver, nigdy wolną ścieżkę. */
import type { ReactElement } from 'react';
import { useEffect, useState } from 'react';
import type { Agent } from '../../state/agents';
import { useWorkspaces } from '../../state/workspaces';
import { why } from '../../ipc/why';
import { listSkillSources } from './io';
import type { SkillSources } from './io';

export function SkillSourcePicker({
  value,
  onChange,
}: {
  readonly value: Agent;
  readonly onChange: (next: Agent) => void;
}): ReactElement | null {
  const folder = useWorkspaces(
    (state) => state.all.find((one) => one.id === state.activeId)?.folder ?? null,
  );
  const [listed, setListed] = useState<SkillSources[]>([]);
  const [said, setSaid] = useState<string | null>(null);
  const names = JSON.stringify(value.skills);
  useEffect(() => {
    let current = true;
    setListed([]);
    setSaid(null);
    const selected: string[] = JSON.parse(names) as string[];
    if (selected.length > 0) {
      void listSkillSources(folder, selected)
        .then((sources) => {
          if (current) setListed(sources);
        })
        .catch((error: unknown) => {
          if (current) setSaid(why(error, 'The selected skill sources could not be read.'));
        });
    }
    return () => {
      current = false;
    };
  }, [folder, names]);
  if (value.skills.length === 0) return null;
  return (
    <div className="stack" data-gap="2" aria-label="Selected skill sources">
      {said === null ? null : (
        <p role="alert" className="lead">
          {said}
        </p>
      )}
      {listed.map((skill) => {
        const selected = value.skillSources?.[skill.name] ?? '';
        return (
          <div key={skill.name} className="stack" data-gap="1">
            <label className="label" htmlFor={`skill-source-${skill.name}`}>
              Source for {skill.name}
            </label>
            <select
              id={`skill-source-${skill.name}`}
              className="field"
              value={selected}
              onChange={(event) => {
                const skillSources = { ...value.skillSources };
                if (event.target.value === '') delete skillSources[skill.name];
                else skillSources[skill.name] = event.target.value;
                onChange({ ...value, skillSources });
              }}
            >
              <option value="">
                {skill.requiresChoice ? 'Choose a source' : 'Use the first matching source'}
              </option>
              {skill.sources.map((source) => (
                <option key={source.path} value={source.path} disabled={!source.available}>
                  {source.path}
                </option>
              ))}
            </select>
            {skill.requiresChoice && selected === '' ? (
              <p className="lead">Choose which copy of this skill to use.</p>
            ) : null}
            {selected !== '' &&
            !skill.sources.some((source) => source.path === selected && source.available) ? (
              <p className="lead">
                The saved source is unavailable in this project. Choose an available source before
                running.
              </p>
            ) : null}
            {skill.sources.length === 0 ? (
              <p className="lead">No source for this skill was found.</p>
            ) : null}
            <details>
              <summary className="caption">Files and delivery</summary>
              <p className="caption">
                The complete selected folder is copied for the agent. Copying a helper does not run
                it. Available files do not prove that the agent used the skill.
              </p>
              <p className="caption">
                Up to 10,000 entries, 1 MiB per file, and 5 MiB per skill. Missing resources and
                links outside the skill folder are refused.
              </p>
              {skill.sources.map((source) => (
                <p key={source.path} className="caption break-all">
                  {source.path} —{' '}
                  {source.available
                    ? `${String(source.files)} entries; ${String(source.bytes)} bytes; ${source.digest?.slice(0, 8) ?? ''}`
                    : (source.reason ?? 'This source cannot be read.')}
                </p>
              ))}
              <p className="caption">
                Loadout checks whether the agent app can receive the selected files before starting.
                An unsupported agent app is reported separately.
              </p>
            </details>
          </div>
        );
      })}
    </div>
  );
}
