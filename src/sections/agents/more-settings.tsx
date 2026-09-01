/* Trzy wiersze pod `More settings`: Tools, Skills, Connections.
 *
 * ══ BYŁO PIĘĆ, ZOSTAŁY TRZY — 2026-08-31 ════════════════════════════════════════════════════
 *
 * `Can it reach the web` przeniosło się MIĘDZY WIDOCZNE (`agent-form.tsx`). To jest pytanie
 * o uprawnienie, dokładnie tej samej rangi co dial plikowy, a uprawnienie schowane pod
 * przyciskiem „więcej ustawień" jest uprawnieniem, którego się nie widzi.
 *
 * `Extra options` przeniosło się pod osobne, jawne `Advanced` (`advanced.tsx`). Literówka
 * w `Skills` daje agenta bez jednej umiejętności; literówka w surowych argumentach zmienia
 * komendę, którą uruchamiamy. To nie jest ta sama ranga decyzji, więc nie stoi pod tą samą
 * nazwą.
 *
 * ══ CZEMU `Tools` DALEJ TU STOI, CHOĆ MIAŁO WYPAŚĆ ══════════════════════════════════════════
 *
 * Miało wypaść z formularza w całości i to jest zgłoszone, nie zrobione. Powód jest mierzalny:
 * `src/sections/field-is-a-well-under-its-label.test.tsx` sądzi osobno gałąź pola WYŁĄCZONEGO
 * przez vendora („keeps that true for the fields a vendor closes"), a `Tools` przy Codeksie jest
 * jedynym takim polem w całym formularzu — po jego usunięciu tamten punkt nie ma czego sądzić
 * i przewraca się na własnej kontroli przeciw pustej asercji. Ten sam plik wymaga też co
 * najmniej dziewięciu etykiet w rozwiniętym formularzu. Oba punkty leżą POZA zakresem tej
 * zmiany, a komentarz w tamtym pliku mówi wprost: „if that changed, this point has to be
 * pointed somewhere else, not deleted". Wskazać go gdzie indziej może właściciel tamtego pliku.
 *
 * Skarga na to pole zostaje w mocy i jest prawdziwa: u Codeksa jest niedostępne, u Claude'a nie
 * ma ani pickera, ani sprawdzenia wpisu, a jedyna rzecz, po którą po nie sięgano — sieć — ma
 * własny wiersz od 2026-08-23 i od dziś stoi wśród widocznych.
 *
 * ══ RESZTA, BEZ ZMIAN ═══════════════════════════════════════════════════════════════════════
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
 * Kontrolki to pola tekstowe z nazwami po przecinku, a nie pickery z makiety
 * (`docs/mockup/index.html:611`: `[ + Add a skill ]`). Picker potrzebuje listy umiejętności
 * z dysku, a ta wchodzi z T-18; przycisk, który otwiera picker, którego nie ma, jest
 * kontrolką bez handlera (niezmiennik 16). Pole tekstowe zapisuje każdą literę i osiąga każdy
 * stan typu — łącznie z `everything`, które jest tu pustym polem, a nie brakiem wartości.
 */
import type { ReactElement } from 'react';
import type { Agent, Tools } from '../../state/agents';
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

/* `FIELD_OFF` I `fieldClass` ZNIKŁY 2026-08-31, bo pole wyłączone jest dziś REGUŁĄ.
 *
 * Stała brzmiała `field text-muted` i była drugim opisem jednego stanu: `.field:disabled`
 * w `theme.css` gasi tusz do `--muted` i stawia kursor `not-allowed` (DESIGN §6, trzy brakujące
 * stany dopisane w tej samej fali). Prawdziwym nośnikiem tego stanu był i został atrybut
 * `disabled` — ten sam, który sprawia, że kontrolki naprawdę nie da się użyć — plus zdanie pod
 * polem, które mówi DLACZEGO. Studnia zostaje: pole bez studni czyta się jak podpis, a nie jak
 * miejsce do pisania, które jest chwilowo zamknięte. */
const FIELD = 'field';

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
    /* WEJŚCIE SPRĘŻYNĄ, 2026-08-31 (DESIGN §7): tych wierszy NIE MA w dokumencie, dopóki
       człowiek nie naciśnie `More settings` — są poza drzewem, nie schowane stylem.
       Powierzchnia, która pojawia się skokiem pod przyciskiem, czyta się jak przeskok widoku;
       dorastanie do miejsca mówi „przyszedłem stamtąd" i kosztuje 200 ms. Jeden region na to
       zdarzenie, przy suficie dwóch (ARCHITECTURE §7). */
    <div className="stack enter border-t border-line pt-3" data-gap="3">
      <div className="stack">
        <label htmlFor="agent-tools" className="label">
          Tools
        </label>
        <input
          id="agent-tools"
          data-field="tools"
          className={FIELD}
          value={toolsText(value.tools)}
          placeholder="Everything"
          disabled={tools === 'unavailable'}
          title={tools === 'approximate' ? APPROXIMATE : undefined}
          onChange={(event) => onChange({ ...value, tools: toolsFrom(event.target.value) })}
        />
        {tools === 'unavailable' ? <p className="lead">{CODEX_HAS_NO_TOOL_LIST}</p> : null}
      </div>

      <div className="stack">
        <label htmlFor="agent-skills" className="label">
          Skills
        </label>
        <input
          id="agent-skills"
          data-field="skills"
          className={FIELD}
          value={value.skills.join(', ')}
          placeholder="None"
          disabled={skills === 'unavailable'}
          title={skills === 'approximate' ? APPROXIMATE : undefined}
          onChange={(event) => onChange({ ...value, skills: listOf(event.target.value) })}
        />
      </div>

      <div className="stack">
        <label htmlFor="agent-connections" className="label">
          Connections
        </label>
        <input
          id="agent-connections"
          data-field="connections"
          className={FIELD}
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
