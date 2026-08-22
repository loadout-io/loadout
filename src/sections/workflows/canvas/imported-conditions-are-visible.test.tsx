import { describe, expect, it } from 'vitest';
import { importedConditionLabel, toCanvas } from './map';
import type { WorkflowFile } from '../../../state/workflows';

describe('Imported workflow conditions', () => {
  it('uses visible English wording from persisted edge data', () => {
    expect(importedConditionLabel({ source: 'check', outcome: 'passed' })).toBe('When checks pass');
    expect(importedConditionLabel({ source: 'checkpoint', choice: 'Ship it' })).toBe(
      'When you choose Ship it',
    );
    const file = {
      format: 1,
      id: 'imported',
      name: 'Imported',
      steps: [],
      links: [{ from: 'check', to: 'ship' }],
      linkConditions: [{ from: 'check', to: 'ship', when: { source: 'check', outcome: 'passed' } }],
    } as WorkflowFile;
    expect(toCanvas(file).edges[0]?.condition).toEqual({ source: 'check', outcome: 'passed' });
  });
});
