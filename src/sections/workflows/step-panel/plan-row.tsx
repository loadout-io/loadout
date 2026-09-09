/* WP-02: Plan jest jednym wierszem opartym na resolverze Rusta, z wyborem za ujawnieniem.
 *
 * 2026-09-09 — tryb stoi nad pickerem, bo właściciel wyniósł stan Plan na wierzch panelu;
 * człowiek ma go widzieć bez otwierania ani głównej pokrywy, ani samego pickera.
 *
 * 2026-09-08 — ten komponent pokazuje nazwę źródłowego kroku, nigdy przewidywany numer wersji.
 * Numer istnieje dopiero po publikacji; pokazany wcześniej byłby wiarygodnie wyglądającą fikcją. */
import type { ReactElement } from 'react';

import type { StepPlan, StepPlanMode, StepPlanView } from '../../../state/workflows';
import { Tick } from '../../../ui/primitives/tick';

const MODES: ReadonlyArray<{ value: StepPlanMode; label: string }> = [
  { value: 'off', label: 'Off' },
  { value: 'create', label: 'Create' },
  { value: 'update', label: 'Update' },
  { value: 'use', label: 'Use' },
];

const SECTIONS = ['Implementation', 'Design', 'Validation'] as const;

function modeOf(value: StepPlan | undefined, view: StepPlanView | null): StepPlanMode | null {
  if (value === undefined) return view?.mode === 'use' ? 'use' : 'off';
  return MODES.find((mode) => mode.value === value.mode)?.value ?? null;
}

function withMode(value: StepPlan | undefined, mode: StepPlanMode): StepPlan | undefined {
  if (mode === 'off') return { mode };
  if (mode === 'create') return { mode };
  if (mode === 'update') {
    return {
      mode,
      canUpdate: value?.mode === 'update' ? value.canUpdate : [],
      samePlanAs: value?.mode === 'use' || value?.mode === 'update' ? value.samePlanAs : undefined,
    };
  }
  return {
    mode,
    focusOn: value?.mode === 'use' ? value.focusOn : [],
    samePlanAs: value?.mode === 'use' || value?.mode === 'update' ? value.samePlanAs : undefined,
  };
}

function toggled(selected: readonly string[], section: string): string[] {
  return selected.includes(section)
    ? selected.filter((one) => one !== section)
    : [...selected, section];
}

function withSource(
  value: StepPlan | undefined,
  mode: StepPlanMode,
  samePlanAs: string | undefined,
): StepPlan | undefined {
  const changed = withMode(value, mode);
  return changed === undefined ? undefined : { ...changed, samePlanAs };
}

export interface PlanRowProps {
  value: StepPlan | undefined;
  view: StepPlanView | null;
  refusal: string | null;
  onChoose: (value: StepPlan | undefined) => void;
}

export function PlanRow({ value, view, refusal, onChoose }: PlanRowProps): ReactElement {
  const mode = modeOf(value, view);
  const label = MODES.find((one) => one.value === mode)?.label ?? 'Needs attention';
  const canUpdate =
    mode === 'update' && (value?.canUpdate?.length ?? 0) > 0 ? (value?.canUpdate ?? []) : SECTIONS;
  const focusOn = mode === 'use' ? (value?.focusOn ?? []) : [];
  const said = view?.said ?? refusal;

  return (
    <div data-row="plan" className="stack">
      <span className="label">Plan</span>
      <span className="lead">Plan: {label}</span>
      {view?.source === null || view?.source === undefined ? null : (
        <p className="lead">{view.source.said}</p>
      )}
      {said === null ? null : <p className="text-body text-warn">{said}</p>}

      <details data-step-plan-picker className="rounded-sm border border-line p-2">
        <summary className="caption cursor-pointer">Plan · {label}</summary>
        <div className="stack pt-2" data-gap="2">
          {MODES.map((option) => (
            <label key={option.value} className="flex items-baseline gap-2 text-body text-ink">
              <input
                type="radio"
                name="step-plan-mode"
                checked={mode === option.value}
                onChange={() => onChoose(withMode(value, option.value))}
              />
              {option.label}
            </label>
          ))}
        </div>
      </details>

      {mode === 'update' ? (
        <details className="rounded-sm border border-line p-2">
          <summary className="caption cursor-pointer">Can update</summary>
          <div className="stack pt-2" data-gap="2">
            {SECTIONS.map((section) => (
              <Tick
                key={section}
                className="flex items-baseline gap-2 text-body text-ink"
                label={section}
                checked={canUpdate.includes(section)}
                onChange={() => {
                  const next = toggled(canUpdate, section);
                  onChoose({
                    mode: 'update',
                    canUpdate: next.length === SECTIONS.length ? [] : next,
                    samePlanAs: value?.samePlanAs,
                  });
                }}
              />
            ))}
          </div>
        </details>
      ) : null}

      {mode === 'use' ? (
        <details className="rounded-sm border border-line p-2">
          <summary className="caption cursor-pointer">Focus on (optional)</summary>
          <div className="stack pt-2" data-gap="2">
            {SECTIONS.map((section) => (
              <Tick
                key={section}
                className="flex items-baseline gap-2 text-body text-ink"
                label={section}
                checked={focusOn.includes(section)}
                onChange={() => {
                  onChoose({
                    mode: 'use',
                    focusOn: toggled(focusOn, section),
                    samePlanAs: value?.samePlanAs,
                  });
                }}
              />
            ))}
            <p className="caption">
              Focus changes emphasis only. The complete shared core and all human requirements still
              apply.
            </p>
          </div>
        </details>
      ) : null}

      {mode === 'update' || mode === 'use' ? (
        (view?.earlier.length ?? 0) > 0 ? (
          <details className="rounded-sm border border-line p-2">
            <summary className="caption cursor-pointer">Same plan as</summary>
            <div className="stack pt-2" data-gap="2">
              <label className="flex items-baseline gap-2 text-body text-ink">
                <input
                  type="radio"
                  checked={value?.samePlanAs === undefined}
                  onChange={() => onChoose(withSource(value, mode, undefined))}
                />
                From dependencies
              </label>
              {view?.earlier.map((source) => (
                <label key={source.stepId} className="flex items-baseline gap-2 text-body text-ink">
                  <input
                    type="radio"
                    checked={value?.samePlanAs === source.stepId}
                    onChange={() => onChoose(withSource(value, mode, source.stepId))}
                  />
                  {source.name}
                </label>
              ))}
            </div>
          </details>
        ) : null
      ) : null}
    </div>
  );
}
