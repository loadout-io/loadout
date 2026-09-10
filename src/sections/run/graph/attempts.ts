import type { GraphStep, Plan } from './model';

// 2026-09-10: rozwinięcie pętli należy do silnika. Kolumna pokazuje zapisany kafelek
// i jego wykonania; identyczna nazwa nie dowodzi, że dwa kafelki są jednym krokiem.
export function groupedSteps(plan: Plan) {
  const groups = new Map<string, GraphStep[]>();
  const physical = new Map(plan.steps.map((step) => [step.id, step]));
  for (const step of plan.steps) {
    const key = step.tileId || step.id;
    const group = groups.get(key) ?? [];
    group.push(step);
    groups.set(key, group);
  }
  const rows = [...groups].map(([id, attempts]) => {
    const started = attempts.filter(
      (step) =>
        step.processStarted === true ||
        (step.processStarted === undefined &&
          !step.notRun &&
          !['waiting', 'skipped'].includes(step.status)),
    );
    const current =
      attempts.find((step) => ['working', 'needs you'].includes(step.status)) ??
      attempts.find((step) => step.waitingForHeavy) ??
      // QA z poprzedniej rundy nie jest wynikiem nowej rundy. Gdy jej poprzednik
      // już pracuje lub skończył, pokaż oczekujące wykonanie, a nie dawny sukces.
      attempts.find(
        (step) =>
          step.status === 'waiting' &&
          !step.notRun &&
          plan.links.some((link) => {
            if (link.to !== step.id || link.max_turns !== undefined) return false;
            const parent = physical.get(link.from);
            return (
              parent !== undefined &&
              !parent.notRun &&
              (parent.waitingForHeavy ||
                ['working', 'needs you', 'done', 'failed'].includes(parent.status))
            );
          }),
      ) ??
      [...attempts].reverse().find((step) => !step.notRun && step.status !== 'waiting') ??
      attempts[0]!;
    return {
      id,
      step: { ...current, id },
      currentId: current.id,
      attempts: started,
      repeated: attempts.length > 1,
    };
  });
  const logical = new Map(plan.steps.map((step) => [step.id, step.tileId || step.id]));
  const firstAttempts = new Map([...groups].map(([id, steps]) => [id, steps[0]!.id]));
  const seen = new Set<string>();
  const links = plan.links.flatMap((link) => {
    const from = logical.get(link.from) ?? link.from;
    const to = logical.get(link.to) ?? link.to;
    // Krawędź do kolejnego wykonania wraca w pętli; nie jest nowym poprzednikiem kafelka.
    if (firstAttempts.has(to) && link.to !== to && link.to !== firstAttempts.get(to)) return [];
    const key = `${from}:${to}:${link.max_turns ?? ''}`;
    if (from === to || seen.has(key)) return [];
    seen.add(key);
    return [{ ...link, from, to }];
  });
  return { rows, plan: { steps: rows.map((row) => row.step), links } };
}
