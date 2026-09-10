import { useEffect, useState } from 'react';
import type { Vendor } from '../state/agents';
import { listAgentModels, type ModelCatalog } from '../state/agent-models';

export function ModelPicker({
  id,
  vendor,
  value,
  onChange,
  initialCatalog,
}: {
  id: string;
  vendor: Vendor;
  value: string;
  onChange: (value: string) => void;
  initialCatalog?: ModelCatalog;
}) {
  const [result, setResult] = useState<{ vendor: Vendor; catalog: ModelCatalog } | null>(
    initialCatalog ? { vendor, catalog: initialCatalog } : null,
  );
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError('');
    setResult(null);
    void listAgentModels(vendor)
      .then((catalog) => {
        if (alive) setResult({ vendor, catalog });
      })
      .catch(() => {
        if (alive) setError('Could not check models. Check sign-in and refresh.');
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [vendor, revision]);
  const catalog = result?.vendor === vendor ? result.catalog : null;
  const choices = catalog?.models.filter((m) => !m.hidden) ?? [];
  const selected = catalog?.models.find((m) => m.id === value || m.aliases.includes(value));
  const missing = !!catalog && value.trim() !== '' && !selected;
  return (
    <div className="stack" data-model-picker>
      <div className="flex items-center gap-2">
        <label htmlFor={id} className="label">
          Model
        </label>
        <button
          type="button"
          className="btn-bare ml-auto"
          disabled={loading}
          onClick={() => setRevision((n) => n + 1)}
        >
          Refresh models
        </button>
      </div>
      <input
        id={id}
        data-field="model"
        className="field"
        list={`${id}-choices`}
        value={value}
        placeholder="App default"
        aria-invalid={missing}
        aria-describedby={`${id}-status`}
        onChange={(e) => onChange(e.target.value)}
      />
      <datalist id={`${id}-choices`}>
        {choices.map((m) => (
          <option key={m.id} value={m.id}>
            {m.displayName}
            {m.isDefault ? ' · Recommended' : ''}
          </option>
        ))}
      </datalist>
      <p id={`${id}-status`} className="lead" role={missing || error ? 'alert' : undefined}>
        {loading
          ? 'Checking models…'
          : error ||
            (missing
              ? `${value} is not offered by ${vendor === 'codex' ? 'Codex' : 'Claude Code'}. Choose a model below before running.`
              : selected?.description || 'Models are checked again before the workflow starts.')}
      </p>
      {choices.length > 0 && (
        <div className="grid gap-2 @xl:grid-cols-2" aria-label="Available models">
          {choices.map((m) => (
            <button
              type="button"
              key={m.id}
              className="row flex flex-col items-start gap-1 border border-line text-left"
              aria-pressed={selected?.id === m.id}
              onClick={() => onChange(m.id)}
            >
              <span className="text-ink">
                {m.displayName}
                {m.isDefault ? ' · Recommended' : ''}
              </span>
              <span className="lead">{m.description}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
