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
      /* Oba kroki MUSZĄ tu stać: od 2026-08-22 mapper nie rysuje strzałki, której koniec nie
         istnieje (leczenie plików z wiszącą strzałką, `map.ts`). Dokument z samą strzałką i bez
         kroków nie jest zresztą niczym, co mogłoby wyjść z importu. */
      steps: [
        { kind: 'checkpoint', id: 'check', name: 'Check', at: { x: 24, y: 24 } },
        { kind: 'checkpoint', id: 'ship', name: 'Ship', at: { x: 24, y: 168 } },
      ],
      links: [{ from: 'check', to: 'ship' }],
      linkConditions: [{ from: 'check', to: 'ship', when: { source: 'check', outcome: 'passed' } }],
    } as WorkflowFile;
    expect(toCanvas(file).edges[0]?.condition).toEqual({ source: 'check', outcome: 'passed' });
  });
});
