/* Kontrakt ręcznego wydania. Runbook jest dokumentacją, ale jego kolejność i polecenia są częścią
 * procesu tak samo jak kod: pominięcie Gatekeepera albo smoke lokalnego DMG daje zielony opis i
 * niezweryfikowany asset. Dlatego test czyta wyłącznie numerowane kroki oraz jawnie oznaczone
 * bloki poleceń, zamiast szukać słów w całym Markdownie.
 *
 * 2026-08-31 — brakujący `docs/RELEASE.md` jest pustym tekstem, nie błędem odczytu. Pierwszy
 * przebieg ma skompilować ten plik i paść na asercji o krokach; błąd importu nie dowodzi braku
 * kontraktu wydania (AGENTS.md §2a).
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

type Step = Readonly<{
  number: number;
  title: string;
  body: string;
}>;

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const RELEASE = resolve(ROOT, 'docs', 'RELEASE.md');
const RUNBOOK = existsSync(RELEASE) ? readFileSync(RELEASE, 'utf8') : '';

const SAFE_LABEL = 'Komendy bezpieczne do skopiowania:';
const OPERATOR_LABEL = 'Placeholder operatora — uzupełnij lokalnie, nie kopiuj wprost:';
const words = (...parts: string[]): string => parts.join(' ');

const EXPECTED_TITLES = [
  'Zamroź czyste drzewo i zapisz SHA',
  'Sprawdź wersje i identyfikator',
  'Uruchom pełną bramkę dla zapisanego SHA',
  'Zbuduj artefakty',
  'Podpisz artefakty certyfikatem Developer ID',
  'Wyślij DMG do notaryzacji',
  'Dołącz poświadczenie notaryzacji',
  'Sprawdź Gatekeeper niezależnie',
  words('Policz', 'SHA-256', 'DMG'),
  'Opublikuj tag, release i pobierz asset',
  'Zainstaluj pobrany DMG i wykonaj smoke',
  'Przekaż sposób aktualizacji',
] as const;

function numberedSteps(markdown: string): Step[] {
  const headings = [...markdown.matchAll(/^## (?<number>\d+)\. (?<title>[^\n]+)$/gm)];

  return headings.map((heading, index) => {
    const next = headings.at(index + 1);
    const start = (heading.index ?? 0) + heading[0].length;
    const end = next?.index ?? markdown.length;

    return {
      number: Number(heading.groups?.number ?? 0),
      title: heading.groups?.title?.trim() ?? '',
      body: markdown.slice(start, end),
    };
  });
}

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Czyta tylko ogrodzenie stojące pod właściwą etykietą; przypadkowy przykład nie jest krokiem. */
function labelledBlocks(step: Step, label: string, language: 'bash' | 'text'): string[] {
  const pattern = new RegExp(
    escaped(label) + '\\n\\n```' + language + '\\n([\\s\\S]*?)\\n```',
    'g',
  );
  return [...step.body.matchAll(pattern)].map((match) => match[1] ?? '');
}

/** Komentarz powłoki nie jest wykonaniem; sądzimy wyłącznie niepuste linie poleceń. */
function safeCommands(step: Step): string[] {
  return labelledBlocks(step, SAFE_LABEL, 'bash')
    .flatMap((block) => block.split('\n'))
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#'));
}

function operatorText(step: Step): string {
  return labelledBlocks(step, OPERATOR_LABEL, 'text').join('\n');
}

function requiredStates(step: Step): string[] {
  return [...step.body.matchAll(/^- Stan wymagany:[^\n]*(?:\n {2,}[^\n]*)*/gm)].map((match) =>
    (match[0] ?? '').replace(/\n\s+/g, ' '),
  );
}

function command(step: Step, exact: string): void {
  expect(
    safeCommands(step),
    `krok ${String(step.number)} musi wykonywać dokładnie: ${exact}`,
  ).toContain(exact);
}

function stateIncludes(step: Step, fragment: string): void {
  expect(
    requiredStates(step).some((state) => state.includes(fragment)),
    `krok ${String(step.number)} nie zapisuje wymaganego stanu: ${fragment}`,
  ).toBe(true);
}

describe('manual release runbook', () => {
  it('binds one clean-tree SHA through a downloaded-DMG installation smoke', () => {
    const steps = numberedSteps(RUNBOOK);

    expect(
      steps.map((step) => [step.number, step.title]),
      'docs/RELEASE.md musi mieć komplet kolejnych, numerowanych kroków ręcznego wydania',
    ).toEqual(EXPECTED_TITLES.map((title, index) => [index + 1, title]));

    const [clean, metadata, gate, build, sign, notarize, staple, gatekeeper, hash, publish, smoke] =
      steps;
    expect(clean).toBeDefined();
    expect(metadata).toBeDefined();
    expect(gate).toBeDefined();
    expect(build).toBeDefined();
    expect(sign).toBeDefined();
    expect(notarize).toBeDefined();
    expect(staple).toBeDefined();
    expect(gatekeeper).toBeDefined();
    expect(hash).toBeDefined();
    expect(publish).toBeDefined();
    expect(smoke).toBeDefined();

    command(clean!, 'test -z "$(git status --porcelain)"');
    command(clean!, 'RELEASE_SHA="$(git rev-parse --verify HEAD^{commit})"');
    command(clean!, 'test -n "$RELEASE_SHA"');
    stateIncludes(clean!, 'konkretny, pełny SHA');

    command(
      metadata!,
      words('PACKAGE_VERSION="$(node', '-p', '"require(\'./package.json\').version")"'),
    );
    command(
      metadata!,
      'CARGO_VERSION="$(sed -n \'s/^version = "\\(.*\\)"$/\\1/p\' src-tauri/Cargo.toml | head -n 1)"',
    );
    command(
      metadata!,
      words('TAURI_VERSION="$(node', '-p', '"require(\'./src-tauri/tauri.conf.json\').version")"'),
    );
    command(
      metadata!,
      words(
        'TAURI_IDENTIFIER="$(node',
        '-p',
        '"require(\'./src-tauri/tauri.conf.json\').identifier")"',
      ),
    );
    command(metadata!, 'test "$PACKAGE_VERSION" = "$CARGO_VERSION"');
    command(metadata!, 'test "$PACKAGE_VERSION" = "$TAURI_VERSION"');
    command(metadata!, 'test "$TAURI_IDENTIFIER" = "com.loadout.desktop"');

    command(gate!, 'test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"');
    command(gate!, 'scripts/ci.sh full');
    expect(
      safeCommands(gate!).at(-1),
      'pełna bramka musi kończyć się ponownym związaniem SHA',
    ).toBe('test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"');

    command(build!, 'test "$(git rev-parse --verify HEAD^{commit})" = "$RELEASE_SHA"');
    command(build!, 'npm run app:build');
    command(build!, 'test -d "$APP"');
    command(build!, 'test -f "$DMG"');

    expect(operatorText(sign!), 'podpis jest świadomą czynnością operatora').toContain(
      'Developer ID Application',
    );
    expect(operatorText(sign!), 'kod musi być podpisany od środka, bez --deep').toContain(
      'od środka na zewnątrz',
    );
    expect(operatorText(sign!), 'runbook musi jawnie odmawiać podpisywania przez --deep').toContain(
      'nie używaj --deep',
    );
    command(sign!, 'codesign --verify --strict --verbose=4 "$APP"');
    command(sign!, 'codesign --verify --strict --verbose=4 "$DMG"');
    stateIncludes(sign!, 'Developer ID Application');

    expect(
      operatorText(notarize!),
      'notaryzacja pozostaje interaktywną czynnością operatora',
    ).toContain(words('xcrun', 'notarytool', 'submit', '"$DMG"', '--wait'));
    stateIncludes(notarize!, 'Accepted');

    command(staple!, 'xcrun stapler staple "$DMG"');
    command(staple!, 'xcrun stapler validate "$DMG"');

    command(
      gatekeeper!,
      'spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG"',
    );
    command(
      gatekeeper!,
      'spctl --assess --type execute --verbose=4 "$GATEKEEPER_MOUNT/Loadout.app"',
    );
    stateIncludes(gatekeeper!, 'niezależna');
    stateIncludes(gatekeeper!, 'accepted');

    command(hash!, 'DMG_SHA256="$(shasum -a 256 "$DMG" | awk \'{print $1}\')"');
    command(hash!, 'test "$(printf \'%s\' "$DMG_SHA256" | wc -c | tr -d \' \')" -eq 64');
    stateIncludes(hash!, '64-znakowy');

    expect(operatorText(publish!), 'tag musi wskazywać zapisany SHA').toContain(
      'git tag -a "$RELEASE_TAG" "$RELEASE_SHA"',
    );
    expect(operatorText(publish!), 'tag musi trafić do zdalnego repozytorium').toContain(
      'git push origin "$RELEASE_TAG"',
    );
    expect(operatorText(publish!), 'release musi opublikować ten DMG').toContain(
      'gh release create "$RELEASE_TAG" "$DMG"',
    );
    command(
      publish!,
      'REMOTE_TAG_SHA="$(git ls-remote origin "refs/tags/$RELEASE_TAG^{}" | awk \'{print $1}\')"',
    );
    command(publish!, 'test "$REMOTE_TAG_SHA" = "$RELEASE_SHA"');
    command(
      publish!,
      'gh release download "$RELEASE_TAG" --pattern \'*.dmg\' --dir "$DOWNLOAD_DIR"',
    );
    command(
      publish!,
      'DOWNLOADED_SHA256="$(shasum -a 256 "$DOWNLOADED_DMG" | awk \'{print $1}\')"',
    );
    command(publish!, 'test "$DOWNLOADED_SHA256" = "$DMG_SHA256"');

    command(
      smoke!,
      'hdiutil attach "$DOWNLOADED_DMG" -nobrowse -readonly -mountpoint "$SMOKE_MOUNT"',
    );
    command(smoke!, 'ditto "$SMOKE_MOUNT/Loadout.app" "$SMOKE_APPS/Loadout.app"');
    command(smoke!, 'open "$SMOKE_APPS/Loadout.app"');
    stateIncludes(smoke!, 'pobranego DMG');
    stateIncludes(smoke!, 'główne okno');

    expect(RUNBOOK).toContain(
      'Obecnie aktualizacja wymaga ręcznego pobrania nowego DMG i ponownego zainstalowania aplikacji.',
    );

    const safe = steps.flatMap((step) => safeCommands(step)).join('\n');
    expect(safe, 'kopiowalne komendy nie mogą zawierać placeholderów operatora').not.toMatch(
      /<[A-ZĄĆĘŁŃÓŚŹŻa-ząćęłńóśźż][^>\n]*>/,
    );
    expect(
      RUNBOOK,
      'runbook nie przechowuje profilu, identyfikatorów zespołu ani nazw sekretów',
    ).not.toMatch(
      /APPLE[_ -]?ID|TEAM[_ -]?ID|NOTARY[_ -]?PROFILE|KEYCHAIN[_ -]?PROFILE|API[_ -]?KEY|PASSWORD|TOKEN|SECRET/i,
    );
    expect(RUNBOOK, 'runbook nie tworzy workflow wydania').not.toContain('.github/workflows');
  });
});
