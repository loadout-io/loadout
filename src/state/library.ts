/** Wspólny kształt odpowiedzi obu półek biblioteki — lustro `library::definition` w Ruście. */
export type DefinitionShelf = 'agents' | 'workflows';

/** Zamknięte kategorie, z których ekran składa własne, angielskie zdanie. */
export type DefinitionProblemKind =
  'unreadable' | 'malformed' | 'newerFormat' | 'missingFormat' | 'olderFormat';

export interface Healthy<T> {
  kind: 'healthy';
  value: T;
}

export interface DefinitionProblem {
  kind: 'definitionProblem';
  shelf: DefinitionShelf;
  /** Sama nazwa pliku. Nigdy ścieżka absolutna ani fragment jego treści. */
  fileName: string;
  problem: DefinitionProblemKind;
}

export type Definition<T> = Healthy<T> | DefinitionProblem;

/** Tymczasowa zgodność dla wstrzykiwanych adapterów starszych testów; IPC oddaje tylko union. */
export type DefinitionListing<T> = readonly (Definition<T> | T)[];

function isDefinition<T>(value: Definition<T> | T): value is Definition<T> {
  return typeof value === 'object' && value !== null && 'kind' in value;
}

/** Jedna normalizacja wejścia — produkcja już niesie union, stare atrapy niosą zdrową wartość. */
export function definitionsOf<T>(listed: DefinitionListing<T>): Definition<T>[] {
  return listed.map((entry) => (isDefinition(entry) ? entry : { kind: 'healthy', value: entry }));
}

/** Zdrowe wartości dla callerów, którzy nie renderują biblioteki. */
export function healthyOnly<T>(definitions: readonly Definition<T>[]): T[] {
  return definitions.flatMap((definition) =>
    definition.kind === 'healthy' ? [definition.value] : [],
  );
}

/** Problemy dla prawdziwego ekranu półki. */
export function definitionProblems<T>(definitions: readonly Definition<T>[]): DefinitionProblem[] {
  return definitions.flatMap((definition) =>
    definition.kind === 'definitionProblem' ? [definition] : [],
  );
}

/** Jedno angielskie zdanie z zamkniętej kategorii; parser ani ścieżka nie docierają do okna. */
export function problemSays(problem: DefinitionProblem): string {
  const namedThing = problem.shelf === 'agents' ? 'an agent' : 'a workflow';
  const folder = problem.shelf === 'agents' ? 'Agents' : 'Workflows';
  const manual = `Open your ${folder} folder to repair or remove it, then reload.`;
  switch (problem.problem) {
    case 'unreadable':
      return `Loadout could not read “${problem.fileName}”. ${manual}`;
    case 'newerFormat':
      return `“${problem.fileName}” was saved by a newer Loadout. Update Loadout, or open your ${folder} folder to remove it.`;
    case 'missingFormat':
      return `“${problem.fileName}” does not say which Loadout format wrote it. ${manual}`;
    case 'olderFormat':
      return `“${problem.fileName}” uses a Loadout format this version can no longer open. ${manual}`;
    case 'malformed':
      return `“${problem.fileName}” is not ${namedThing} Loadout can read. ${manual}`;
  }
}
