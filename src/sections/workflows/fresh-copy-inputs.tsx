/* WF-14: wzorce są decyzją człowieka. Podgląd niczego nie zapisuje, a osobne zatwierdzenie
 * zapisuje te same wzorce przez zwykły autosave dokumentu. Treści plików nie wracają do okna. */
import type { ReactElement } from 'react';
import { useEffect, useRef, useState } from 'react';
import { why } from '../../ipc/why';
import { useWorkspaces } from '../../state/workspaces';
import { previewAdditionalInputs } from './io';
import type { AdditionalInputPreview } from './io';

export function FreshCopyInputs({
  value,
  onChoose,
}: {
  readonly value: readonly string[];
  readonly onChoose: (patterns: string[]) => void;
}): ReactElement {
  const folder = useWorkspaces(
    (state) => state.all.find((one) => one.id === state.activeId)?.folder ?? null,
  );
  const saved = value.join('\n');
  const [draft, setDraft] = useState(saved);
  const [preview, setPreview] = useState<AdditionalInputPreview | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    generation.current += 1;
    setDraft(saved);
    setPreview(null);
    setSaid(null);
    setBusy(false);
    return () => {
      generation.current += 1;
    };
  }, [saved, folder]);

  const patterns = draft
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '');
  async function showPreview(): Promise<void> {
    if (folder === null) return;
    const ticket = ++generation.current;
    setBusy(true);
    setSaid(null);
    setPreview(null);
    try {
      const found = await previewAdditionalInputs(folder, patterns);
      if (ticket === generation.current) setPreview(found);
    } catch (error) {
      if (ticket === generation.current)
        setSaid(why(error, 'The selected files could not be read.'));
    } finally {
      if (ticket === generation.current) setBusy(false);
    }
  }

  return (
    <details className="shrink-0 border-b border-line px-4 py-2">
      <summary className="label">Files in fresh copies</summary>
      <section aria-label="Fresh copy file selection" className="stack max-w-200 py-3" data-gap="2">
        <p className="lead">
          Every fresh copy starts from one frozen input. Tracked files keep current edits;
          additional untracked or ignored files require an explicit choice. Environment files are
          not included automatically.
        </p>
        <label className="label" htmlFor="additional-input-patterns">
          Additional input patterns
        </label>
        <textarea
          id="additional-input-patterns"
          className="field font-mono"
          rows={3}
          value={draft}
          onChange={(event) => {
            generation.current += 1;
            setDraft(event.target.value);
            setPreview(null);
            setBusy(false);
            setSaid(null);
          }}
        />
        <p className="caption">
          One relative path or glob per line, for this whole workflow. Directories include their
          files. Patterns must match at least one file inside this project. Dependency folders such
          as node_modules and target are not included.
        </p>
        {folder === null ? (
          <p className="lead">Choose a project folder to preview additional files.</p>
        ) : null}
        {said === null ? null : (
          <p role="alert" className="lead" data-tone="attend">
            {said}
          </p>
        )}
        <div className="flex gap-2">
          <button
            type="button"
            className="btn-quiet"
            disabled={busy || folder === null}
            onClick={() => {
              void showPreview();
            }}
          >
            Preview selected files
          </button>
          <button
            type="button"
            className="btn"
            disabled={preview === null || busy}
            onClick={() => {
              onChoose(patterns);
            }}
          >
            Use these input patterns
          </button>
        </div>
        {preview === null ? null : (
          <>
            <p className="caption">
              {preview.files.length} entries · {preview.totalBytes} bytes. Additional input limit:{' '}
              {preview.fileLimit} files and {Math.round(preview.byteLimit / 1024 / 1024)} MiB.
            </p>
            {preview.files.some((file) => file.private || file.ignored) ? (
              <p className="lead" data-tone="attend">
                Selected private or ignored files may contain secrets. Using these patterns gives
                their contents to the workflow copies and the selected agent apps.
              </p>
            ) : null}
            <ul className="stack max-h-48 overflow-auto" data-gap="1">
              {preview.files.map((file) => (
                <li key={file.path} className="caption break-all">
                  {file.path} — {file.bytes} bytes{file.ignored ? ' · ignored by Git' : ''}
                  {file.private ? ' · private environment file' : ''}
                </li>
              ))}
            </ul>
          </>
        )}
        <p className="caption">
          Prepare dependencies with an explicit Check in a fresh copy and continue in the same
          files. Use a real environment test with a pass count. Separate fresh copies need separate
          preparation; after combining branches, run another Check if dependencies are needed.
          Removing that graph step removes preparation.
        </p>
      </section>
    </details>
  );
}
