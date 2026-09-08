import type { ReactElement } from 'react';
import { useState } from 'react';

import type { ContextFinding, ContextRevision, RevisionEdit } from '../../state/context';

export interface ContextOverviewProps {
  revision: ContextRevision;
  onSave: (edit: RevisionEdit) => void;
}

export default function ContextOverview({ revision, onSave }: ContextOverviewProps): ReactElement {
  const [correction, setCorrection] = useState('');
  const questions = revision.findings.filter((finding) => finding.kind === 'question');
  const conflicts = revision.findings.filter(
    (finding) => finding.kind === 'conflict' || finding.conflictsWith.length > 0,
  );

  return (
    <div data-context-overview className="flex flex-col gap-4">
      <section className="card flex flex-col gap-2">
        <h3 className="text-heading text-ink">Short index</h3>
        <ul className="flex flex-col gap-1">
          {revision.topics.map((topic) => (
            <li key={topic.id} className="value">
              {topic.title}
            </li>
          ))}
        </ul>
      </section>

      {revision.topics.map((topic) => (
        <section key={topic.id} data-context-topic={topic.id} className="card flex flex-col gap-2">
          <h3 className="text-heading text-ink">{topic.title}</h3>
          {revision.findings
            .filter(
              (finding) =>
                finding.topic === topic.id &&
                finding.kind !== 'question' &&
                finding.kind !== 'conflict' &&
                finding.conflictsWith.length === 0,
            )
            .map((finding) => (
              <FindingRow key={finding.id} finding={finding} onSave={onSave} />
            ))}
        </section>
      ))}

      <SeparatedFindings
        title="Questions"
        findings={questions}
        extra={revision.questions}
        onSave={onSave}
      />
      <SeparatedFindings
        title="Conflicts"
        findings={conflicts}
        extra={revision.conflicts}
        onSave={onSave}
      />

      <section className="card flex flex-col gap-2">
        <label className="label" htmlFor="context-correction">
          Add a correction for the next build
        </label>
        <textarea
          id="context-correction"
          className="field"
          value={correction}
          onChange={(event) => {
            setCorrection(event.target.value);
          }}
        />
        <button
          data-save-correction
          type="button"
          className="btn"
          disabled={correction.trim() === ''}
          onClick={() => {
            onSave({ correction: correction.trim(), findingId: null, text: null });
            setCorrection('');
          }}
        >
          Save as a new version
        </button>
      </section>
    </div>
  );
}

function FindingRow({
  finding,
  onSave,
}: {
  readonly finding: ContextFinding;
  readonly onSave: (edit: RevisionEdit) => void;
}): ReactElement {
  const [text, setText] = useState(finding.text);
  return (
    <div data-context-finding={finding.id} className="flex flex-col gap-1">
      <div className="flex items-center gap-2">
        <input
          aria-label="Finding"
          className="field flex-1"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
          }}
        />
        <button
          type="button"
          className="btn-quiet"
          disabled={text.trim() === '' || text === finding.text}
          onClick={() => {
            onSave({ correction: '', findingId: finding.id, text: text.trim() });
          }}
        >
          Save finding
        </button>
        {finding.origin === 'human' ? <span className="value">Yours</span> : null}
      </div>
      {finding.condition === '' ? null : <span className="lead">When: {finding.condition}</span>}
      <span className="lead">
        Sources: {finding.sources.map((source) => `${source.sourceId} ${source.part}`).join(', ')}
      </span>
    </div>
  );
}

function SeparatedFindings({
  title,
  findings,
  extra,
  onSave,
}: {
  readonly title: string;
  readonly findings: readonly ContextFinding[];
  readonly extra: readonly string[];
  readonly onSave: (edit: RevisionEdit) => void;
}): ReactElement {
  const represented = new Set(findings.map((finding) => finding.text));
  const remaining = [...new Set(extra.filter((line) => !represented.has(line)))];
  if (findings.length === 0 && remaining.length === 0) return <></>;
  return (
    <section className="card flex flex-col gap-2">
      <h3 className="text-heading text-ink">{title}</h3>
      {findings.map((finding) => (
        <FindingRow key={finding.id} finding={finding} onSave={onSave} />
      ))}
      {remaining.map((line) => (
        <p key={line} className="text-ink">
          {line}
        </p>
      ))}
    </section>
  );
}
