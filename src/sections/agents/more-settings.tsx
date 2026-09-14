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
 * `Tools` i `Skills` to dalej pola tekstowe z nazwami po przecinku, a nie pickery z makiety
 * (`docs/mockup/index.html:611`: `[ + Add a skill ]`). Picker potrzebuje listy z dysku, a lista
 * umiejętności wchodzi z T-18; przycisk, który otwiera picker, którego nie ma, jest kontrolką
 * bez handlera (niezmiennik 16). Pole tekstowe zapisuje każdą literę i osiąga każdy stan typu —
 * łącznie z `everything`, które jest tu pustym polem, a nie brakiem wartości.
 *
 * ══ `Connections` MA JUŻ SWÓJ PICKER — 2026-09-14 ═══════════════════════════════════════════
 *
 * Blokada z akapitu wyżej zniknęła dla tego jednego wiersza: `conn-seed` dał oknu nazwy
 * połączeń WŁĄCZONYCH w bibliotece projektu, więc jest z czego wybierać. Pole tekstowe stąd
 * ZNIKŁO, nie zeszło pod listę — pole i lista mówiące o tym samym fakcie to dwa żywe regiony
 * jednego faktu (niezmiennik 13), a to z nich, które przyjmuje literówkę, jest tym gorszym.
 * Etykieta i jej `id` zostają bez zmian; kontrolkę pod nimi rysuje `./connection-picker.tsx`.
 */
import type { ReactElement } from 'react';
import type { Agent, Tools } from '../../state/agents';
import { Tick } from '../../ui/primitives/tick';
import { capability } from './capabilities';
import { ConnectionPicker } from './connection-picker';
import { SkillSourcePicker } from '../skills/source-picker';
import { AppPermissions } from './app-permissions';

export interface MoreSettingsProps {
  value: Agent;
  onChange: (next: Agent) => void;
  /**
   * Otwiera import połączeń. Podany WYŁĄCZNIE wtedy, gdy biblioteka tego projektu na pewno nie
   * ma ani jednego włączonego połączenia — odczyt się udał i oddał zero (`./index.tsx`).
   */
  onImportConnections?: () => void;
}

/** Jedno zdanie i dokładnie to zdanie [T4 §8.1]. */
const CODEX_HAS_NO_TOOL_LIST =
  "Codex doesn't have this. It uses the 'Can change files' setting instead.";

/** Podpowiedź pod kursorem przy polu, które druga aplikacja tłumaczy na najbliższą swoją
 * rzecz [T4 §6.1: przybliżenie to zwykła kontrolka plus jedna linia]. */
const APPROXIMATE = 'Codex has this, but sets it up its own way.';

/* SKĄD WZIĄĆ POŁĄCZENIA, KIEDY TEN PROJEKT NIE MA ŻADNEGO — 2026-09-13.
 *
 * Nowy agent startuje od tego dnia z połączeniami włączonymi w bibliotece projektu, a biblioteka
 * projektu jest świeża (od 2026-09-10 nie korzysta z domyślnej biblioteki użytkownika) — więc
 * puste pole bez słowa zostawiało właściciela z pytaniem, skąd te nazwy w ogóle wziąć.
 *
 * ZDANIE NAZYWA DWA ŹRÓDŁA, bo żaden z dwóch importów nie prowadzi do obu. `Import setup`
 * sekcji Agents czyta to, czego Claude Code i Codex używają w folderze (`.mcp.json`,
 * `~/.claude.json`, serwery Codeksa); `Import setup from project` z przełącznika projektów kopiuje
 * z innej biblioteki Loadouta, w tym z „Previous shared library", czyli ze starego `~/.loadout`,
 * gdzie leżą połączenia właściciela. Przycisk otwiera pierwszy, bo to okno tej sekcji: wstaje
 * bez przemontowania ekranu, a import z przełącznika podbija `setupRevision` w kluczu osłony
 * (`src/App.tsx`) i wyrzuciłby niezapisanego nowego agenta. Etykieta jest inna niż
 * „Import setup", bo dwie specyfikacje e2e klikają w tej sekcji przycisk po tym napisie. */
const NO_CONNECTIONS_HERE =
  'No connections in this project yet: import the tool servers Claude Code or Codex already use ' +
  'here, or copy them from another project or the previous shared library with Import setup ' +
  'from project in the project menu.';

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

export function MoreSettings({
  value,
  onChange,
  onImportConnections,
}: MoreSettingsProps): ReactElement {
  const tools = capability('tools', value.runsWith);
  const skills = capability('skills', value.runsWith);
  /* `capability('connections', …)` NIE JEST tu czytane od 2026-09-14. Tabela odpowiada `native`
     przy OBU aplikacjach, więc obie gałęzie („wygaś" i „podpowiedz") były nieosiągalne, a niosły
     kopie zdań widocznych niżej. Powód i droga powrotu stoją w nagłówku `./connection-picker.tsx`. */

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
        <SkillSourcePicker value={value} onChange={onChange} />
      </div>

      <div className="stack">
        <label htmlFor="agent-connections" className="label">
          Connections
        </label>
        <ConnectionPicker
          value={value}
          onChange={onChange}
          describedBy={onImportConnections === undefined ? undefined : 'agent-connections-where'}
        />
        {/* POD LISTĄ, NIGDY ZAMIAST NIEJ, i tylko z handlerem (niezmiennik 16): bez propsu nie
            ma tu ani zdania, ani przycisku. `lead`, nie `label` — tekst w etykiecie stałby się
            nazwą kontrolki, a wiersze formularza sądzi się po klasie `label`. */}
        {onImportConnections === undefined ? null : (
          <>
            <p id="agent-connections-where" className="lead">
              {NO_CONNECTIONS_HERE}
            </p>
            <button type="button" className="btn-quiet self-start" onClick={onImportConnections}>
              Import tool servers
            </button>
          </>
        )}
      </div>
      <AppPermissions value={value} onChange={onChange} />
      <Tick
        className="flex items-center gap-2"
        label="Allow messages between steps"
        checked={value.agentMessages ?? false}
        onChange={(event) => onChange({ ...value, agentMessages: event.target.checked })}
      />
    </div>
  );
}
