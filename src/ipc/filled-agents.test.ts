import { describe, expect, it } from 'vitest';
import { filledAgents, whatTheyGot } from './filled-agents';

describe('what an import handed to the agents', () => {
  it('says which connections went to which agents', () => {
    expect(
      whatTheyGot([
        { agent: 'lead', connections: ['figma', 'linear-server'] },
        { agent: 'picky', connections: ['figma', 'linear-server'] },
      ]),
    ).toBe('Gave figma and linear-server to lead and picky.');
  });

  it('names a few agents and counts the rest', () => {
    const many = ['lead', 'picky', 'runner', 'scout', 'tester', 'writer', 'reader'].map(
      (agent) => ({ agent, connections: ['figma'] }),
    );

    expect(
      whatTheyGot(many),
      'seven names in one line is a list nobody reads; three plus a count is still true',
    ).toBe('Gave figma to lead, picky, runner and 4 more.');
  });

  it('says nothing at all when no agent got anything', () => {
    expect(whatTheyGot([])).toBeNull();
    expect(
      whatTheyGot([{ agent: 'lead', connections: [] }]),
      'an agent that got nothing is not news, and a sentence about it would be a lie',
    ).toBeNull();
  });

  it('reads the list off the wire and keeps only entries of the right shape', () => {
    expect(
      filledAgents([
        { agent: 'lead', connections: ['figma'] },
        { agent: 'broken', connections: 'figma' },
        'not an entry at all',
      ]),
      'invoke<T> is a cast, not a check: one answer of the wrong shape may not take the screen ' +
        'down with it',
    ).toEqual([{ agent: 'lead', connections: ['figma'] }]);
    expect(filledAgents(undefined)).toEqual([]);
    expect(filledAgents({ agent: 'lead' })).toEqual([]);
  });
});
