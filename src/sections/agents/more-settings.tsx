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
/* POLE BIERZE KLASE DOMU, NIE WLASNY OPIS.
 *
 * `theme.css` ma klase `.field` od pierwszego dnia: studnia, mocny obrys, promien z pasma, kroj
 * maszynowy i `user-select: text` — to ostatnie jest czescia pola, nie ozdoba, bo `body` wylacza
 * zaznaczanie w calej aplikacji. Do 2026-08-19 wolaly ja DWA miejsca, a cztery sekcje przepisywaly
 * ten sam wyglad recznie w dwunastu stalych — i rozjechaly sie: tu obrys byl `--line`, w Skills
 * `--line-strong`. Jeden fakt, jedno miejsce (niezmiennik 13); dwa opisy tego samego pola czyta
 * sie jak dwa rozne stany, a nie jak dwa pola.
 *
 * Skupienia tu nie ma z tego samego powodu. `theme.css` daje `.field:focus` obwodke w akcencie
 * i globalny `:focus-visible` obrys — jedna regula na cala aplikacje. Dopisanie tego samego
 * narzedziem na kazdym polu byloby trzecia kopia decyzji, ktora juz jest podjeta. */
const FIELD = 'field';
/* WYLACZONE POLE ZOSTAJE POLEM. Do 2026-08-19 stalo tu `field bg-panel text-muted`, czyli klasa
 * domu z NADPISANYM tlem — a wtedy jedyna kontrolka, ktora Codex wylacza (`Tools`), rysowala sie
 * bez studni. Pole bez studni czyta sie jak podpis, nie jak pole: znika informacja, ze to jest
 * miejsce do pisania, ktore w tym ukladzie jest chwilowo zamkniete. Zostaje wiec studnia, gasnie
 * tylko tusz — plus atrybut `disabled`, ktory jest prawdziwym nosnikiem tego stanu, i zdanie pod
 * polem, ktore mowi DLACZEGO. */
const FIELD_OFF = 'field text-muted';

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
