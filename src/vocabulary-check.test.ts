import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, it } from 'vitest';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Każdy przypadek uruchamia prawdziwy check nad osobnym repo, nie jego kopię w teście. */
function judge(file: string, source: string): { status: number | null; output: string } {
  const fixture = mkdtempSync(join(tmpdir(), 'loadout-vocabulary-'));
  try {
    cpSync(join(ROOT, 'checks'), join(fixture, 'checks'), { recursive: true });
    symlinkSync(join(ROOT, 'node_modules'), join(fixture, 'node_modules'));
    const target = join(fixture, file);
    mkdirSync(dirname(target), { recursive: true });
    writeFileSync(target, source);
    const result = spawnSync('bash', ['checks/vocabulary.sh'], {
      cwd: fixture,
      encoding: 'utf8',
      timeout: 20_000,
    });
    if (result.error !== undefined) throw result.error;
    return { status: result.status, output: result.stdout + result.stderr };
  } finally {
    rmSync(fixture, { recursive: true, force: true });
  }
}

it.each([
  [
    'src/view.tsx',
    'const View = () => <>{ready ? <p>Ready</p> : snapshot !== "" ? <p>Saved</p> : null}</>;',
  ],
  [
    'src/view.tsx',
    'const text = `${String(source.files)} files; ${source.digest?.slice(0, 8) ?? ""}`;',
  ],
  [
    'src/view.test.tsx',
    'it("the session uses the correct snapshot", () => expect(command).toBe("node test.cjs"));',
  ],
  ['src/view.tsx', "// The agent's session is internal.\nconst View = () => <p>Ready</p>;"],
])('accepts non-visible code in %s', (file, source) => {
  const result = judge(file, source);
  expect(result.status, result.output).toBe(0);
});

it.each([
  ['src/view.tsx', 'const View = () => <p>Start a new session</p>;'],
  ['src/view.tsx', 'const View = () => <button aria-label="Session">Start</button>;'],
  ['src/view.tsx', 'const View = () => <button aria-label={"Session"}>Start</button>;'],
  ['src/view.ts', 'export const said = "Start a new session";'],
  ['src/view.tsx', 'const View = () => <p>{`Start a new session for ${agent.name}`}</p>;'],
  ['src/view.tsx', 'const View = () => <p>{ready ? "Ready" : "Start a new session"}</p>;'],
  ['src-tauri/src/error.rs', 'fn fail() { Error::Other("Start a new session"); }'],
])('refuses real wording in %s', (file, source) => {
  const result = judge(file, source);
  expect(result.status, result.output).toBe(1);
  expect(result.output).toContain('jargon reached text a user can read');
});
