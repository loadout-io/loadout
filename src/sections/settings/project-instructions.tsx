/* WF-12: jawny wybór w aktywnym projekcie. Otwarcie Settings niczego nie włącza ani nie
 * zapisuje; odpowiedź starego workspace nie może przestawić aktualnie widocznego wyboru. */
import type { ReactElement } from 'react';
import { useEffect, useRef, useState } from 'react';

import { why } from '../../ipc/why';
import { readProjectSettings, saveProjectSettings } from '../../state/settings-io';
import type { ProjectSettings, ProjectSettingsPatch } from '../../state/settings-io';
import { useWorkspaces } from '../../state/workspaces';

export function ProjectInstructions(): ReactElement | null {
  const folder = useWorkspaces(
    (state) => state.all.find((one) => one.id === state.activeId)?.folder ?? null,
  );
  if (folder === null) return null;
  return <ProjectInstructionsForFolder key={folder} folder={folder} />;
}

function ProjectInstructionsForFolder({ folder }: { readonly folder: string }): ReactElement {
  const [settings, setSettings] = useState<ProjectSettings | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    void readProjectSettings(folder)
      .then((value) => {
        if (mounted.current) setSettings(value);
      })
      .catch((error: unknown) => {
        if (mounted.current) setSaid(why(error, 'Project instructions could not be read.'));
      });
    return () => {
      mounted.current = false;
    };
  }, [folder]);

  async function save(patch: ProjectSettingsPatch): Promise<void> {
    setSaving(true);
    setSaid(null);
    try {
      const value = await saveProjectSettings(folder, patch);
      if (mounted.current) setSettings(value);
    } catch (error) {
      if (mounted.current) setSaid(why(error, 'Project instructions could not be saved.'));
    } finally {
      if (mounted.current) setSaving(false);
    }
  }

  return (
    <section aria-label="Project instructions" className="card mt-3 max-w-200 stack" data-gap="3">
      <h2 className="text-heading text-ink">Project instructions</h2>
      <p className="caption break-all">{folder}</p>
      {said === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {said}
        </p>
      )}
      {settings === null ? (
        <p className="lead">Reading this project’s settings…</p>
      ) : (
        <>
          <div className="flex items-center gap-2">
            <input
              id="project-instructions-enabled"
              type="checkbox"
              checked={settings.instructions.enabled}
              disabled={saving}
              onChange={(event) => {
                void save({ instructions: { enabled: event.target.checked } });
              }}
            />
            <label htmlFor="project-instructions-enabled" className="label">
              Use project instructions
            </label>
          </div>
          <p className="lead">
            Applies to new workflows. Each workflow keeps its original instructions; the lead reads
            the current project at the next message. Individual steps and the lead can override this
            choice.
          </p>
          <details>
            <summary className="label">Sources and scope</summary>
            <p className="lead">
              Text only. Loadout does not grant permissions or import hooks, environment values, or
              agent-app settings through this option. The agent app may separately load its own
              native project instructions; this choice does not disable that behavior.
            </p>
            <p className="caption">
              Up to {settings.limits.files} files, {Math.round(settings.limits.fileBytes / 1024)}{' '}
              KiB per file, {Math.round(settings.limits.totalBytes / 1024)} KiB total. A selected
              file that cannot be read is refused, not shortened.
            </p>
            <p className="caption">
              Deeper folders specialize parent folders. At the same scope AGENTS.md takes
              precedence. Rule path patterns retain their scope. Includes use a separate @include
              relative/file.md line; cycles and links are refused.
            </p>
            <ul className="stack" data-gap="1">
              {settings.sources.map((source, index) => (
                <li key={`${source.path}:${index}`} className="caption">
                  <span className="break-all">{source.path}</span> — {source.bytes} bytes; folder{' '}
                  {source.directory || '.'}
                  {source.paths.length === 0 ? null : `; paths ${source.paths.join(', ')}`}
                  {source.local ? '; private local source' : null}
                </li>
              ))}
            </ul>
            {settings.sources.length === 0 ? (
              <p className="caption">No project instruction sources found.</p>
            ) : null}
            <div className="flex items-center gap-2 mt-3">
              <input
                id="project-instructions-local"
                type="checkbox"
                checked={settings.instructions.includeLocal}
                disabled={saving}
                onChange={(event) => {
                  void save({ instructions: { includeLocal: event.target.checked } });
                }}
              />
              <label htmlFor="project-instructions-local" className="label">
                Include private CLAUDE.local.md files
              </label>
            </div>
            <p className="caption">
              Local files may contain private instructions. Enabling this sends their selected text
              to the chosen agent app.
            </p>
            <label htmlFor="lead-project-instructions" className="label block mt-3">
              Lead project instructions
            </label>
            <select
              id="lead-project-instructions"
              className="field"
              disabled={saving}
              value={
                settings.leadInstructions === null
                  ? 'inherit'
                  : settings.leadInstructions
                    ? 'on'
                    : 'off'
              }
              onChange={(event) => {
                void save({
                  leadInstructions:
                    event.target.value === 'inherit' ? null : event.target.value === 'on',
                });
              }}
            >
              <option value="inherit">Use project choice</option>
              <option value="on">Always use</option>
              <option value="off">Do not use</option>
            </select>
          </details>
        </>
      )}
    </section>
  );
}
