/* Nazwy argumentów, których Rust oczekuje dla danej komendy — czytane WPROST z `ipc.rs`.
 *
 * # Po co to istnieje
 *
 * Tauri dopasowuje argumenty **po nazwie** i deserializuje je **przed** wejściem w ciało
 * komendy. Skutek jest brutalny i cichy: literówka w kluczu nie daje mniejszego wywołania,
 * daje ODRZUCONE. Front odbija się przy każdym kliknięciu, a odmowa przychodzi surowym napisem,
 * którego nikt nie widzi. Tak był zepsuty Start 2026-08-17 (brakujący `lines`), tak była
 * zepsuta kontrolka „dalej" (brakujący `answer`) i tak samo umarłoby pisanie do agenta, gdyby
 * `say_to_agent` dostało `{ said }` zamiast `{ text }`.
 *
 * # Dlaczego lista NIE jest tu przepisana
 *
 * Bo wtedy byłaby drugim miejscem, w którym mieszka odpowiedź na pytanie „jak nazywa się ten
 * argument" (niezmiennik 13), i rozjechałaby się w dniu, w którym Rust doda parametr. Ten plik
 * nie zna ANI JEDNEJ nazwy argumentu — zna tylko drogę do pliku, który je trzyma.
 *
 * # Skąd tu przyszedł
 *
 * Dwa kryteria z 2026-08-18 (`run/start-args-complete.test.tsx` i
 * `run/continue-at-checkpoint.test.tsx`) noszą własne, identyczne kopie tego parsera. Ten moduł
 * jest jego jedną wersją dla wszystkiego, co powstaje później; tamte dwie kopie zostają na razie
 * tam, gdzie są, i to jest **zgłoszenie, nie przeoczenie**: przepisanie ich nie zmienia ani
 * jednej asercji, więc jest porządkami, a nie naprawą, i nie chowa się po cichu w zmianie
 * o czymś innym.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** `src-tauri/src/ipc.rs` — jedyne miejsce, w którym stoją skorupy komend. */
const IPC = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src-tauri/src/ipc.rs');

/**
 * Treść `ipc.rs`, albo pusty napis, jeśli pliku nie ma.
 *
 * Pusty napis, a nie wyjątek: wołający ma o niego zapytać JAWNIE i powiedzieć człowiekowi, że
 * jego zbiór oczekiwań przyszedł z niczego. Porównanie dwóch pustych zbiorów przechodzi, i to
 * jest dokładnie ten kształt zieleni, którego te kryteria mają nie mieć.
 */
export function ipcSource(): string {
  try {
    return readFileSync(IPC, 'utf8');
  } catch {
    return '';
  }
}

/** Wnętrze listy parametrów funkcji o podanej nazwie. */
function signature(rust: string, fn: string): string {
  const at = rust.indexOf(`fn ${fn}(`);
  if (at < 0) return '';
  const from = rust.indexOf('(', at);
  let depth = 0;
  for (let i = from; i < rust.length; i += 1) {
    const ch = rust[i];
    if (ch === '(') depth += 1;
    else if (ch === ')') {
      depth -= 1;
      if (depth === 0) return rust.slice(from + 1, i);
    }
  }
  return '';
}

/** Dzieli listę parametrów po przecinkach NA POZIOMIE ZERO — `State<'_, AppState>` ma własny. */
function parameters(inside: string): readonly string[] {
  const out: string[] = [];
  let depth = 0;
  let current = '';
  for (const ch of inside) {
    if (ch === '<' || ch === '(' || ch === '[') depth += 1;
    else if (ch === '>' || ch === ')' || ch === ']') depth -= 1;
    if (ch === ',' && depth === 0) {
      out.push(current);
      current = '';
    } else current += ch;
  }
  out.push(current);
  return out.map((one) => one.trim()).filter((one) => one !== '');
}

/** `how_many_at_once` → `howManyAtOnce`, bo tak Tauri przepisuje nazwy w stronę okna. */
function camel(snake: string): string {
  return snake.replace(/_([a-z])/g, (_all, letter: string) => letter.toUpperCase());
}

/**
 * Nazwy argumentów, które OKNO ma wysłać dla tej komendy, pod nazwami, których Tauri oczekuje.
 *
 * Odpada wyłącznie to, co Tauri wstrzykuje samo: parametr typu `State<…>`, uchwyt aplikacji
 * i okno. Wszystko inne jest kluczem, którego brak kończy się odrzuconym wywołaniem.
 */
export function windowSideArguments(rust: string, fn: string): readonly string[] {
  return parameters(signature(rust, fn))
    .filter((one) => !/:\s*State\s*</.test(one))
    .filter((one) => !/:\s*(?:tauri::)?(?:AppHandle|Window|WebviewWindow)\b/.test(one))
    .map((one) => camel(one.split(':')[0]?.trim() ?? ''))
    .filter((name) => name !== '');
}
