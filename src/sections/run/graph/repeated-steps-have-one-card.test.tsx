import { renderToStaticMarkup } from 'react-dom/server';
import { expect, it } from 'vitest';
import { RunGraph } from './graph';
import type { GraphStep } from './model';

it('shows one logical tile, its actual run count, and named dependencies', () => {
  const steps: (GraphStep & { tileId?: string; processStarted?: boolean })[] = [
    { id: 'design', name: 'Design', status: 'done' },
    { id: 'c1', tileId: 'combine', name: 'Combine', status: 'failed', processStarted: true },
    { id: 'c2', tileId: 'combine', name: 'Combine', status: 'working', processStarted: true },
    { id: 'c3', tileId: 'combine', name: 'Combine', status: 'waiting', processStarted: false },
    { id: 'qa', name: 'QA', status: 'waiting' },
  ];
  const html = renderToStaticMarkup(
    <RunGraph
      plan={{
        steps,
        links: [
          { from: 'design', to: 'combine' },
          { from: 'combine', to: 'qa' },
        ],
      }}
    />,
  );
  expect(html.match(/data-step="/g)).toHaveLength(3);
  expect(html).toContain('2 runs');
  expect(html).toContain('working');
  expect(html).toContain('after Design');
  expect(html).toContain('after Combine');
  expect(html).not.toContain('runs before');
  expect(html).not.toContain('Run 3');
});

it('keeps different workflow tiles with the same name separate', () => {
  const html = renderToStaticMarkup(
    <RunGraph
      plan={{
        steps: [
          { id: 'a', name: 'Combine', status: 'done' },
          { id: 'b', name: 'Combine', status: 'waiting' },
        ],
        links: [],
      }}
    />,
  );
  expect(html.match(/data-step="/g)).toHaveLength(2);
});
