/* Stopka mówi o KAŻDEJ z czterech odpowiedzi, osobno dla każdej z dwóch aplikacji.
 *
 * PODMIOTEM JEST MARKUP, NIE FUNKCJA SKŁADAJĄCA TEKST (niezmiennik 29). `sentence()` jest czystą
 * funkcją i dowiodłaby wyłącznie tego, że mechanizm istnieje; zdanie w wyrenderowanej stopce
 * dowodzi, że produkt je mówi. Między jednym a drugim mieszka wada, dla której to repo powstało:
 * kryterium zielone, funkcja martwa. Prawdziwe kliknięcie Retry i prawdziwe zwinięcie menu sądzi
 * `e2e/tests/agent-app-preflight-is-visible.spec.ts` — tu jedzie wszystko, co da się rozstrzygnąć
 * bez przeglądarki, czyli KSZTAŁT odpowiedzi granicy.
 *
 * IZOLACJA APLIKACJI JEST OSOBNYM PUNKTEM, bo to jest ta jedna rzecz, którą łatwo napisać źle
 * i której nie widać: jeden `catch` na obie odpowiedzi kasuje poprawny wiersz razem z wadliwym,
 * a stopka mówi wtedy „nie wiem" o aplikacji, która właśnie podała swoją wersję.
 */
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useAgentApps } from '../../state/agent-apps';
import { AgentAppsStatus } from './agent-apps-status';

/* Atrapa granicy, podniesiona razem z `vi.mock`: ten plik mierzy, co okno zrobiło z odpowiedzią,
 * a nie czy Rust ją oddał. Odmowa jest jedną z odpowiedzi i ma tu własny punkt. */
const { answer } = vi.hoisted(() => ({
  answer: { of: undefined as unknown, refuse: false },
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: () => (answer.refuse ? Promise.reject(new Error('boom')) : Promise.resolve(answer.of)),
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const SIGN_IN = 'Sign-in is checked when you first run an agent.';

/** Odpowiedź granicy → to, co po niej widać w stopce. */
async function footerAfter(wire: unknown, collapsed = false): Promise<string> {
  answer.of = wire;
  await useAgentApps.getState().check();
  return renderToStaticMarkup(<AgentAppsStatus collapsed={collapsed} />);
}

/** Markup → to, co człowiek naprawdę czyta. Encje wracają do znaków (`&#x27;` to apostrof). */
function readable(html: string): string {
  return html
    .replace(/<[^>]*>/g, ' ')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, '&');
}

/** Wartość atrybutu ze znacznika otwierającego stopki. */
function attribute(html: string, name: string): string {
  const tag = /<[a-z]+[^>]*data-agent-apps-status[^>]*>/.exec(html)?.[0] ?? '';
  return new RegExp('\\b' + name + '="([^"]*)"').exec(tag)?.[1] ?? '';
}

const BOTH_FOUND = [
  { app: 'claude-code', state: 'found', version: 'claude-code 1.2.3' },
  { app: 'codex', state: 'found', version: 'codex-cli 4.5.6' },
];

beforeEach(() => {
  answer.refuse = false;
  answer.of = undefined;
  useAgentApps.setState({ claudeCode: { state: 'checking' }, codex: { state: 'checking' } });
});

describe('the footer says what the two local agent apps really answered', () => {
  it('starts by admitting nobody has answered yet', () => {
    const said = readable(renderToStaticMarkup(<AgentAppsStatus collapsed={false} />));

    expect(
      said,
      'the footer says nothing about Claude Code before the first answer arrives. Silence here ' +
        'reads as "there is nothing to say", and the honest sentence is that we are asking.',
    ).toContain('Checking Claude Code…');
    expect(said).toContain('Checking Codex…');
    expect(
      said,
      'the footer never admits that being signed in is a different question. A found version ' +
        'without that line reads as "ready", which is the promise this footer stopped making.',
    ).toContain(SIGN_IN);
  });

  it('names the version of each app that answered, and offers nothing to redo', async () => {
    const footer = await footerAfter(BOTH_FOUND);
    const said = readable(footer);

    expect(said, 'Claude Code answered with a version and the footer hides it: ' + said).toContain(
      'Claude Code · claude-code 1.2.3',
    );
    expect(said, 'Codex answered with a version and the footer hides it: ' + said).toContain(
      'Codex · codex-cli 4.5.6',
    );
    expect(
      said,
      'both apps answered and Retry is still standing. A control with nothing left to do is ' +
        'furniture, and furniture teaches people that controls here do nothing.',
    ).not.toContain('Retry');
  });

  it('keeps "not there" and "could not find out" as two different sentences', async () => {
    const said = readable(
      await footerAfter([
        { app: 'claude-code', state: 'not-found' },
        { app: 'codex', state: 'could-not-check' },
      ]),
    );

    expect(said, 'the footer does not say that Claude Code was not found: ' + said).toContain(
      "Claude Code wasn't found.",
    );
    expect(
      said,
      'the footer says Codex is missing, but nobody found that out — the answer was that we ' +
        'could not tell. "Install this" said to somebody who has it installed is the same lie ' +
        'as the old unconditional promise, pointed the other way: ' +
        said,
    ).toContain("Loadout couldn't check Codex.");
    expect(
      said,
      'something is unanswered and there is no way to ask again short of restarting the window.',
    ).toContain('Retry');
  });

  it('does not let one broken answer erase the other app', async () => {
    const said = readable(
      await footerAfter([
        { app: 'claude-code', state: 'nonsense-from-the-future' },
        { app: 'codex', state: 'found', version: 'codex-cli 4.5.6' },
      ]),
    );

    expect(
      said,
      'a state nobody recognises took the other app down with it. Each slot is judged on its ' +
        'own, or one bad installation costs the window everything it knows about both: ' +
        said,
    ).toContain('Codex · codex-cli 4.5.6');
    expect(said).toContain("Loadout couldn't check Claude Code.");
  });

  it('refuses to read an empty version as an answer', async () => {
    const said = readable(
      await footerAfter([
        { app: 'claude-code', state: 'found', version: '   ' },
        { app: 'codex', state: 'found', version: 'codex-cli 4.5.6' },
      ]),
    );

    expect(
      said,
      '"Claude Code · " with nothing after it looks like an answer and is not one. Blank means ' +
        'we did not find out, and that is a sentence this footer already has: ' +
        said,
    ).toContain("Loadout couldn't check Claude Code.");
    expect(said).toContain('Codex · codex-cli 4.5.6');
  });

  it('says it could not find out when the boundary itself refuses', async () => {
    answer.refuse = true;
    const said = readable(await footerAfter(BOTH_FOUND));

    expect(
      said,
      'the read was refused and the footer says both apps are missing. Nothing was learned ' +
        'about either, so blaming the person for an installation they may well have is the one ' +
        'thing this footer must not do: ' +
        said,
    ).toContain("Loadout couldn't check Claude Code.");
    expect(said).toContain("Loadout couldn't check Codex.");
  });

  it('carries every sentence into the tooltip when the column narrows', async () => {
    const footer = await footerAfter(BOTH_FOUND, true);

    expect(
      readable(footer).trim(),
      'the narrowed column still writes the sentences out, in 64 px. A list cut to fit promises ' +
        'readiness for whoever is no longer on it.',
    ).toBe('');
    const tip = readable(attribute(footer, 'title'));
    expect(
      tip,
      'folding the nav threw the answers away instead of moving them. Tooltip says: ' + tip,
    ).toContain('Claude Code · claude-code 1.2.3');
    expect(tip).toContain('Codex · codex-cli 4.5.6');
    expect(
      tip,
      'the folded dot drops the line about signing in, so the tooltip promises more than the ' +
        'wide column does. Tooltip says: ' +
        tip,
    ).toContain(SIGN_IN);
  });
});
