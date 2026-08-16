/* Trzy wiersze pod `More settings`: Tools, Skills, Connections. Trzy i ani jeden więcej.
 *
 * Przy Codeksie `Tools` jest wygaszone i pod spodem stoi jedno zdanie. Bez ikony ostrzeżenia,
 * bez modala, bez czerwieni [T4 §8.1]: to nie jest błąd użytkownika ani awaria, tylko fakt
 * o drugiej aplikacji. Precedens jest cudzy i mocny — `claude import codex --dry-run` mapuje
 * wyłącznie serwery narzędziowe, a resztę wypisuje prostym zdaniem z powodem [T4 §6.2].
 *
 * Który to stan, mówi tabela z `capabilities.ts`, nie ten plik. Warunek `if vendor === 'codex'`
 * postawiony tutaj byłby drugą kopią polityki, a druga kopia zawsze w końcu mówi co innego
 * (niezmiennik 23).
 *
 * Trzy kontrolki to trzy pola tekstowe z nazwami po przecinku, a nie pickery z makiety
 * (`docs/mockup/index.html:611`: `[ + Add a skill ]`). Picker potrzebuje listy umiejętności
 * z dysku, a ta wchodzi z T-18; przycisk, który otwiera picker, którego nie ma, jest
 * kontrolką bez handlera (niezmiennik 16). Pole tekstowe zapisuje każdą literę i osiąga każdy
 * stan typu — łącznie z `everything`, które jest tu pustym polem, a nie brakiem wartości.
 */
import type { ReactElement } from 'react';
import type { Agent, Tools } from '../../state/agents';
import type { Capability } from './capabilities';
import { capability } from './capabilities';

export interface MoreSettingsProps {
  value: Agent;
  onChange: (next: Agent) => void;
}

/** Jedno zdanie i dokładnie to zdanie [T4 §8.1]. */
const CODEX_HAS_NO_TOOL_LIST =
  "Codex doesn't have this. It uses the 'Can change files' setting instead.";

/** Podpowiedź pod kursorem przy polu, które druga aplikacja tłumaczy na najbliższą swoją
 * rzecz [T4 §6.1: przybliżenie to zwykła kontrolka plus jedna linia]. */
const APPROXIMATE = 'Codex has this, but sets it up its own way.';

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
const NOTE = 'text-body text-muted';

/* Stan wygaszenia liczymy w TypeScripcie i podajemy gotową klasę, zamiast wariantu
 * `disabled:` Tailwinda. Powód jest mechaniczny: wariant zostawia w atrybucie `class` słowo
 * `disabled` także wtedy, gdy kontrolka działa — a wtedy „czy ta kontrolka jest wygaszona"
 * przestaje mieć jedną odpowiedź w HTML-u i zaczyna mieć dwie, z których jedna kłamie.
 * Ta sama pułapka stoi w przycisku Save w `agent-form.tsx`. */
const FIELD = 'h-8 rounded-sq border border-line bg-well px-2 text-body text-ink';
const FIELD_OFF = 'h-8 rounded-sq border border-line bg-panel px-2 text-body text-muted';

function fieldClass(state: Capability): string {
  return state === 'unavailable' ? FIELD_OFF : FIELD;
}

/** Nazwy rozdzielone przecinkami -> lista. Puste pole to pusta lista, nigdy `undefined`. */
function listOf(text: string): string[] {
  return text
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

/** Puste pole znaczy „wszystkie narzędzia" — wartość `everything`, nie brak klucza: w RFC 7396
 * brak klucza znaczy „idź za agentem", a to jest zupełnie co innego. */
function toolsFrom(text: string): Tools {
  const only = listOf(text);
  return only.length === 0 ? 'everything' : { only };
}

function toolsText(tools: Tools): string {
  return tools === 'everything' ? '' : tools.only.join(', ');
}

export function MoreSettings({ value, onChange }: MoreSettingsProps): ReactElement {
  const tools = capability('tools', value.runsWith);
  const skills = capability('skills', value.runsWith);
  const connections = capability('connections', value.runsWith);

  return (
    <div className="flex flex-col gap-3 border-t border-line pt-3">
      <div className={ROW}>
        <label htmlFor="agent-tools" className={LABEL}>
          Tools
        </label>
        <input
          id="agent-tools"
          data-field="tools"
          className={fieldClass(tools)}
          value={toolsText(value.tools)}
          placeholder="Everything"
          disabled={tools === 'unavailable'}
          title={tools === 'approximate' ? APPROXIMATE : undefined}
          onChange={(event) => onChange({ ...value, tools: toolsFrom(event.target.value) })}
        />
        {tools === 'unavailable' ? <p className={NOTE}>{CODEX_HAS_NO_TOOL_LIST}</p> : null}
      </div>

      <div className={ROW}>
        <label htmlFor="agent-skills" className={LABEL}>
          Skills
        </label>
        <input
          id="agent-skills"
          data-field="skills"
          className={fieldClass(skills)}
          value={value.skills.join(', ')}
          placeholder="None"
          disabled={skills === 'unavailable'}
          title={skills === 'approximate' ? APPROXIMATE : undefined}
          onChange={(event) => onChange({ ...value, skills: listOf(event.target.value) })}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="agent-connections" className={LABEL}>
          Connections
        </label>
        <input
          id="agent-connections"
          data-field="connections"
          className={fieldClass(connections)}
          value={value.connections.join(', ')}
          placeholder="None"
          disabled={connections === 'unavailable'}
          title={connections === 'approximate' ? APPROXIMATE : undefined}
          onChange={(event) => onChange({ ...value, connections: listOf(event.target.value) })}
        />
      </div>
    </div>
  );
}
