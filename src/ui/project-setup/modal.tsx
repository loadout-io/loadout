import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { why } from '../../ipc/why';
import { useWorkspaces, type Workspace } from '../../state/workspaces';
import { useProjectSetup } from './state';
import './style.css';

interface Item {
  key: string;
  category: string;
  name: string;
  summary: string;
  preview: string;
  requires: string[];
  problems: string[];
  alreadyHere: boolean;
  reusable?: boolean;
}
interface Preview {
  revision: string;
  items: Item[];
}
const categories: Record<string, string> = {
  all: 'All',
  agent: 'Agents',
  workflow: 'Workflows',
  knowledge: 'Knowledge',
  context: 'Context',
  connection: 'Connections',
};
const names: Record<string, string> = {
  agent: 'Agent',
  workflow: 'Workflow',
  skill: 'Skill',
  note: 'Note',
  context: 'Context',
  connection: 'Connection',
};
const glyphs: Record<string, string> = {
  agent: 'A',
  workflow: 'W',
  skill: 'S',
  note: 'N',
  context: 'C',
  connection: '↗',
};

export function ProjectSetupModal() {
  const destination = useProjectSetup((state) => state.destination);
  return destination ? <ImportWindow key={destination.id} destination={destination} /> : null;
}

function ImportWindow({ destination }: { destination: Workspace }) {
  const projects = useWorkspaces((state) => state.all);
  const dialog = useRef<HTMLDialogElement>(null);
  const generation = useRef(0);
  const [source, setSource] = useState<{ name: string; folder: string | null } | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [manual, setManual] = useState<Set<string>>(new Set());
  const [category, setCategory] = useState('all');
  const [query, setQuery] = useState('');
  const [focused, setFocused] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<number | null>(null);
  const close = useProjectSetup.getState().close;

  useEffect(() => {
    const previous = document.activeElement;
    dialog.current?.showModal();
    return () => {
      generation.current += 1;
      if (previous instanceof HTMLElement && previous.isConnected && previous.tabIndex >= 0)
        previous.focus();
      else document.querySelector<HTMLElement>('[data-workspace-open]')?.focus();
    };
  }, []);

  const items = preview?.items ?? [];
  const selection = useMemo(() => {
    const selected = new Set(manual);
    const includedBy = new Map<string, string>();
    const visit = (key: string, owner: string) => {
      const item = preview?.items.find((item) => item.key === key);
      for (const dependency of item?.requires ?? []) {
        if (selected.has(dependency)) continue;
        selected.add(dependency);
        includedBy.set(dependency, owner);
        visit(dependency, owner);
      }
    };
    for (const key of manual)
      visit(key, preview?.items.find((item) => item.key === key)?.name ?? 'your selection');
    return { selected, includedBy };
  }, [manual, preview]);
  const blocked = [...selection.selected].some((key) => {
    const item = items.find((item) => item.key === key);
    return !item || (item.alreadyHere && !item.reusable) || item.problems.length > 0;
  });
  const newCount = items.filter(
    (item) => selection.selected.has(item.key) && !item.reusable,
  ).length;
  const matchesCategory = (item: Item, chosen: string) =>
    chosen === 'all' ||
    item.category === chosen ||
    (chosen === 'knowledge' && ['skill', 'note'].includes(item.category));
  const visible = items.filter(
    (item) =>
      matchesCategory(item, category) &&
      `${item.name} ${item.summary}`.toLowerCase().includes(query.toLowerCase()),
  );
  const detail = items.find((item) => item.key === focused);

  async function chooseSource(next: NonNullable<typeof source>) {
    const ticket = ++generation.current;
    setSource(next);
    setLoading(true);
    setError(null);
    setPreview(null);
    setManual(new Set());
    setFocused(null);
    setQuery('');
    setCategory('all');
    try {
      const answer = await invoke<Preview>('preview_project_setup', {
        folder: destination.folder,
        sourceFolder: next.folder,
      });
      if (ticket === generation.current) setPreview(answer);
    } catch (error) {
      if (ticket === generation.current)
        setError(why(error, 'This project could not be read. Try opening it again.'));
    } finally {
      if (ticket === generation.current) setLoading(false);
    }
  }

  async function importSelected() {
    if (!source || !preview || blocked || busy || newCount === 0) return;
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<{ imported: string[] }>('import_project_setup', {
        folder: destination.folder,
        sourceFolder: source.folder,
        revision: preview.revision,
        selected: [...selection.selected].sort(),
      });
      useProjectSetup.getState().imported(destination.folder);
      setReceipt(result.imported.length);
    } catch (error) {
      setError(why(error, 'The setup could not be imported. Refresh the preview and try again.'));
    } finally {
      setBusy(false);
    }
  }

  return (
    <dialog
      ref={dialog}
      className="setup-dialog"
      aria-labelledby="setup-title"
      onCancel={(event) => {
        if (busy) event.preventDefault();
        else close();
      }}
    >
      <header className="setup-header">
        <div>
          <p className="setup-eyebrow">PROJECT SETUP</p>
          <h2 id="setup-title">Import setup</h2>
          <p>
            Bring selected items into <strong>{destination.name}</strong>.
          </p>
        </div>
        <button
          type="button"
          className="btn"
          aria-label="Close import"
          disabled={busy}
          onClick={close}
        >
          ✕
        </button>
      </header>
      {receipt !== null ? (
        <section className="setup-success" role="status">
          <span className="setup-success-mark" aria-hidden="true">
            ✓
          </span>
          <h3>
            {receipt} {receipt === 1 ? 'item' : 'items'} added to {destination.name}
          </h3>
          <p>Your copies are ready to edit and use in this project.</p>
          <button type="button" className="btn-primary" onClick={close}>
            Done
          </button>
        </section>
      ) : (
        <>
          {!source ? (
            <section className="setup-sources">
              <h3>Choose a project to copy from</h3>
              <p>Pick the agents, workflows and knowledge you want to reuse.</p>
              <div className="setup-source-grid">
                {projects
                  .filter((project) => project.folder !== destination.folder)
                  .map((project) => (
                    <button
                      type="button"
                      className="setup-source"
                      key={project.id}
                      onClick={() => {
                        void chooseSource(project);
                      }}
                    >
                      <span className="setup-glyph" aria-hidden="true">
                        {project.name.slice(0, 2).toUpperCase()}
                      </span>
                      <span>
                        <strong>{project.name}</strong>
                        <small>{project.folder}</small>
                      </span>
                      <span aria-hidden="true">→</span>
                    </button>
                  ))}
              </div>
              <button
                type="button"
                className="setup-legacy"
                onClick={() => {
                  void chooseSource({ name: 'Previous shared library', folder: null });
                }}
              >
                <span>
                  <strong>Previous shared library</strong>
                  <small>Recover setup saved before projects had their own libraries.</small>
                </span>
                <span aria-hidden="true">→</span>
              </button>
            </section>
          ) : (
            <>
              <div className="setup-route">
                <button
                  className="btn"
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    generation.current += 1;
                    setSource(null);
                    setPreview(null);
                    setError(null);
                  }}
                >
                  ← Projects
                </button>
                <span>{source.name}</span>
                <span aria-hidden="true">→</span>
                <strong>{destination.name}</strong>
                <button
                  type="button"
                  className="btn setup-refresh"
                  disabled={loading || busy}
                  onClick={() => {
                    void chooseSource(source);
                  }}
                >
                  Refresh
                </button>
              </div>
              {loading ? (
                <p className="setup-loading" role="status">
                  Reading this project’s setup…
                </p>
              ) : (
                preview && (
                  <>
                    <div className="setup-tools">
                      <div className="setup-filters" aria-label="Item categories">
                        {Object.entries(categories).map(([key, label]) => (
                          <button
                            type="button"
                            className="btn"
                            key={key}
                            aria-pressed={category === key}
                            onClick={() => setCategory(key)}
                          >
                            {label}{' '}
                            <span>{items.filter((item) => matchesCategory(item, key)).length}</span>
                          </button>
                        ))}
                      </div>
                      <input
                        className="field"
                        aria-label="Search setup"
                        placeholder="Find an item…"
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                      />
                    </div>
                    <div className={`setup-browser${detail ? ' has-preview' : ''}`}>
                      <section className="setup-catalog" aria-label="Items to import">
                        <div className="setup-catalog-heading">
                          <span>
                            {visible.length} {visible.length === 1 ? 'item' : 'items'}
                          </span>
                          <button
                            className="btn"
                            type="button"
                            disabled={busy || visible.length === 0}
                            onClick={() =>
                              setManual(
                                new Set([
                                  ...manual,
                                  ...visible
                                    .filter((item) => !item.alreadyHere && !item.problems.length)
                                    .map((item) => item.key),
                                ]),
                              )
                            }
                          >
                            Select shown
                          </button>
                          {manual.size > 0 && (
                            <button
                              className="btn"
                              type="button"
                              disabled={busy}
                              onClick={() => setManual(new Set())}
                            >
                              Clear
                            </button>
                          )}
                        </div>
                        <div className="setup-cards">
                          {visible.map((item) => (
                            <article
                              data-setup-item
                              key={item.key}
                              className="setup-card"
                              data-selected={selection.selected.has(item.key)}
                            >
                              <div className="setup-card-top">
                                <span className="setup-glyph" aria-hidden="true">
                                  {glyphs[item.category] ?? '◇'}
                                </span>
                                <span className="setup-kind">{names[item.category] ?? 'Item'}</span>
                                <input
                                  type="checkbox"
                                  aria-label={`Select ${item.name}`}
                                  checked={selection.selected.has(item.key)}
                                  disabled={
                                    busy ||
                                    item.alreadyHere ||
                                    item.problems.length > 0 ||
                                    selection.includedBy.has(item.key)
                                  }
                                  onChange={(event) =>
                                    setManual((previous) => {
                                      const next = new Set(previous);
                                      if (event.target.checked) next.add(item.key);
                                      else next.delete(item.key);
                                      return next;
                                    })
                                  }
                                />
                              </div>
                              <h3>{item.name}</h3>
                              <p className="setup-card-description">
                                {item.summary || names[item.category]}
                              </p>
                              {selection.includedBy.has(item.key) && (
                                <p className="setup-included">
                                  Included with {selection.includedBy.get(item.key)}
                                </p>
                              )}
                              {item.reusable && (
                                <p className="setup-included">Already here · ready to use</p>
                              )}
                              {item.problems.length > 0 && (
                                <p className="setup-attention">
                                  {item.alreadyHere ? 'Already in this project' : 'Needs attention'}
                                </p>
                              )}
                              <button
                                type="button"
                                className="setup-preview-button"
                                aria-label={`Preview ${item.name}`}
                                onClick={() => setFocused(item.key)}
                              >
                                Preview <span aria-hidden="true">↗</span>
                              </button>
                            </article>
                          ))}
                        </div>
                        {visible.length === 0 && (
                          <div className="setup-empty">
                            <span aria-hidden="true">◇</span>
                            <h3>{items.length ? 'No matching items' : 'Nothing to import yet'}</h3>
                            <p>
                              {items.length
                                ? 'Try another name or category.'
                                : 'Add setup in the source project, then refresh here.'}
                            </p>
                          </div>
                        )}
                      </section>
                      {detail && (
                        <aside className="setup-detail" aria-label={`Preview for ${detail.name}`}>
                          <div className="setup-detail-heading">
                            <span className="setup-kind">{names[detail.category]} PREVIEW</span>
                            <button
                              type="button"
                              className="btn"
                              aria-label="Close preview"
                              onClick={() => setFocused(null)}
                            >
                              ✕
                            </button>
                          </div>
                          <h3>Preview for {detail.name}</h3>
                          <p className="setup-preview-text">{detail.preview || detail.summary}</p>
                          {detail.requires.length > 0 && (
                            <div className="setup-dependencies">
                              <h4>Comes with</h4>
                              {detail.requires.map((key) => (
                                <button
                                  key={key}
                                  type="button"
                                  className="btn"
                                  onClick={() => setFocused(key)}
                                >
                                  {items.find((item) => item.key === key)?.name ?? 'Missing item'}
                                </button>
                              ))}
                            </div>
                          )}
                          {detail.problems.map((problem) => (
                            <p className="setup-attention" key={problem}>
                              {problem}
                            </p>
                          ))}
                        </aside>
                      )}
                    </div>
                  </>
                )
              )}
            </>
          )}
          {error && (
            <p role="alert" className="setup-error">
              {error}
            </p>
          )}
          <footer className="setup-footer">
            <div>
              <strong>{newCount} selected</strong>
              <p>
                {blocked
                  ? 'Some required items need attention. Open their previews to see why.'
                  : 'Copies stay independent. The source project keeps its setup.'}
              </p>
            </div>
            <button
              type="button"
              className="btn-primary"
              disabled={!preview || busy || blocked || newCount === 0}
              onClick={() => {
                void importSelected();
              }}
            >
              {busy ? 'Importing…' : `Import ${newCount} ${newCount === 1 ? 'item' : 'items'}`}
            </button>
          </footer>
        </>
      )}
    </dialog>
  );
}
