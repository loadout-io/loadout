/* Pięć wierszy pod `More settings`: Tools, sieć, Skills, Connections, przelotka do aplikacji.
 *
 * Były trzy i „ani jeden więcej", bo czwarty wiersz jest zawsze obroniony sam z siebie i tak
 * powstaje strona ustawień, której nikt nie wypełnia. Poprzeczka stoi w jednym miejscu (T3 §10,
 * ryzyko 6): nowy element wymaga PRAWDZIWEJ SKARGI. Skarga jest, 2026-08-23: „czemu dostępu do
 * neta nie mają?" — w bibliotece właściciela 18 agentów i ani jeden z siecią, bo z tego
 * formularza nie dało się jej dać. Ten wiersz nie dokłada możliwości; odsłania tę, do której
 * nikt nie trafiał. Powód całego pola stoi przy `Agent::reaches_the_web` w Ruście.
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
import type { Agent, Tools, Vendor } from '../../state/agents';
/* NAZWA APLIKACJI PRZYCHODZI Z TABELI, KTÓRA JĄ JUŻ MA (niezmiennik 13). `VENDORS` napędza
 * wiersz `Runs with` w formularzu obok, a tutaj nazywa wiersz przelotki — druga kopia brzmienia
 * rozjechałaby się przy pierwszej zmianie i nikt by tego nie zauważył, bo nazwa z drutu
 * (`claude-code`) i tak nie ma prawa dotrzeć na ekran (niezmiennik 14).
 *
 * Import wraca do pliku, który ten importuje, i to jest świadome: obie strony czytają się
 * dopiero przy renderowaniu, a nie przy wczytywaniu modułu, więc pierścień nigdy nie sięga po
 * wartość, której jeszcze nie ma. */
import { VENDORS } from './agent-form';
import type { Capability } from './capabilities';
import { capability, webIsOutOfReach } from './capabilities';

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

/** Zdanie pod przełącznikiem sieci. Odpowiada na jedyne pytanie, które w tym miejscu pada. */
const WEB_IS_NOT_ABOUT_FILES =
  'Reading and searching the web only. What it may do with your files stays exactly as set above.';

/* Drugie zdanie pod tym samym przełącznikiem — i tylko wtedy, gdy jest nieprawdą, że włączenie
 * go coś da. Bez ikony ostrzeżenia, bez czerwieni, tak jak zdanie przy `Tools` [T4 §8.1]: to
 * jest fakt o drugiej aplikacji, nie pomyłka człowieka.
 *
 * KTÓRY TO PRZYPADEK, MÓWI TABELA (`capabilities.ts`), nie ten plik. Warunek po nazwie vendora
 * postawiony tutaj byłby drugą kopią polityki, a druga kopia zawsze w końcu mówi co innego
 * (niezmiennik 23). */
const WEB_NEEDS_WRITE_ACCESS =
  'Codex only reaches the web when it can change files, so this agent will not get it.';

/* ── Przelotka: jeden wiersz na parę ──────────────────────────────────────────────────────────
 *
 * PIĄTY WIERSZ WYMAGA PRAWDZIWEJ SKARGI (T3 §10, ryzyko 6) i ta jest zmierzona: przelotka
 * `vendorOptions` jest w formacie agenta od pierwszego dnia, od T-90 naprawdę dojeżdża do argv
 * — i nie dało się jej ustawić inaczej niż edycją pliku na dysku. Ustawienie, do którego nie ma
 * kontrolki, jest ustawieniem, którego nie ma.
 *
 * KLUCZ W PLIKU TO `claude`, NIE `claude-code`. Tak nazywa go plik agenta i tak pyta o niego
 * strona rustowa (`library::agents::vendor_argv`, `workflow::check::reserved`). Wpisanie tu
 * drugiej pisowni dałoby przelotkę, która zapisuje się na ekranie i nie dojeżdża do nikogo.
 */
const PASSTHROUGH_KEY: Record<Vendor, string> = {
  'claude-code': 'claude',
  codex: 'codex',
};

/* KSZTAŁT PARY JEST WŁASNOŚCIĄ APLIKACJI, nie tego pliku, i te dwa są tu LUSTREM strony
 * rustowej — dokładnie tak, jak typy w `src/state/agents.ts` są lustrem tamtejszych struktur.
 * Claude Code bierze `--flaga wartość`, Codex `klucz=wartość`. Jeden kształt dla obu wygląda
 * poprawnie na ekranie i wywala drugą aplikację przy pierwszym prawdziwym biegu. */
const SEPARATOR: Record<Vendor, string> = {
  'claude-code': ' ',
  codex: '=',
};

/** Przykład, którym pole mówi, czego od człowieka chce, zamiast opisywać to zdaniem. */
const LIKE: Record<Vendor, string> = {
  'claude-code': '--fallback-model sonnet',
  codex: 'model_verbosity=high',
};

/** Wiersze z pola → pary do zapisania w pliku. Wiersz bez wartości zostaje z pustą wartością:
 * to on jest odmową zapisu, a wykasowanie go tutaj zabrałoby zdaniu, które ją tłumaczy,
 * wiersz, o którym mówi (`missingForSave` w `src/state/agents.ts`). */
function pairsFrom(text: string, vendor: Vendor): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const one = line.trim();
    if (one.length === 0) continue;
    const at = one.indexOf(SEPARATOR[vendor]);
    const name = at === -1 ? one : one.slice(0, at).trim();
    const value = at === -1 ? '' : one.slice(at + 1).trim();
    if (name.length > 0) out[name] = value;
  }
  return out;
}

/** I z powrotem: pary z pliku → wiersze, w kolejności, w jakiej je zapisano. */
function textFrom(pairs: Record<string, string>, vendor: Vendor): string {
  return Object.entries(pairs)
    .map(([name, value]) => (value === '' ? name : name + SEPARATOR[vendor] + value))
    .join('\n');
}

/** Nazwa aplikacji, którą ten agent biegnie — z tabeli, która ją już ma. */
function appName(vendor: Vendor): string {
  return VENDORS.find((one) => one.value === vendor)?.label ?? vendor;
}

export function MoreSettings({ value, onChange }: MoreSettingsProps): ReactElement {
  const tools = capability('tools', value.runsWith);
  const skills = capability('skills', value.runsWith);
  const connections = capability('connections', value.runsWith);
  const web = capability('reachesTheWeb', value.runsWith);
  /* Tylko kiedy człowiek o sieć POPROSIŁ: zdanie odbierające coś, czego nikt nie chciał,
   * jest szumem, a szum uczy przewijać wzrokiem każdą uwagę w tym formularzu. */
  const webWontReach = value.reachesTheWeb && webIsOutOfReach(value.runsWith, value.fileAccess);

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

      {/* SIEĆ MA WŁASNY WIERSZ, i to jest cała treść tej kontrolki.
       *
       * 2026-08-23 — z pytania właściciela „czemu dostępu do neta nie mają?". Zmierzone w jego
       * bibliotece: 18 agentów, ani jeden z siecią. U Claude'a dało się ją dostać, WPISUJĄC
       * `WebFetch, WebSearch` w pole wyżej — i nikt tego nie zrobił, bo nic o tym nie mówi;
       * u Codeksa to pole jest wygaszone, więc nie dało się w ogóle.
       *
       * Zdanie pod przełącznikiem mówi, czego on NIE robi, bo to jest jedyne, o co człowiek
       * pyta w tym miejscu: „czy przez to zacznie mi ruszać pliki". Nie zacznie — dial mówi
       * o plikach, ten przełącznik o świecie. */}
      <div className={ROW}>
        <label htmlFor="agent-web" className={LABEL}>
          Can it reach the web
        </label>
        {/* LISTA WYBORU, nie przełącznik, i nie z upodobania: wiersz obok — dial dostępu do
            plików — jest `<select>` z klasą domu, a dwa pytania o uprawnienia, zadane dwiema
            różnymi kontrolkami, czytają się jak dwie różne rangi decyzji. Jeden kształt na
            jedną robotę, we wszystkich pięciu sekcjach. */}
        <select
          id="agent-web"
          data-field="reachesTheWeb"
          className={fieldClass(web)}
          value={value.reachesTheWeb ? 'yes' : 'no'}
          onChange={(event) => {
            onChange({ ...value, reachesTheWeb: event.target.value === 'yes' });
          }}
        >
          <option value="no">No</option>
          <option value="yes">Read and search the web</option>
        </select>
        <p className={NOTE}>{WEB_IS_NOT_ABOUT_FILES}</p>
        {webWontReach ? <p className={NOTE}>{WEB_NEEDS_WRITE_ACCESS}</p> : null}
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

      {/* WIERSZ NALEŻY DO APLIKACJI Z `Runs with` i mówi to swoją etykietą. Jedna nazwa, nie
          obie: dwie zamieniłyby jeden wiersz w dwa pytania, a człowiek odpowiedziałby na
          niewłaściwe — kształt pary jest u tych dwóch inny.

          PRZEŁĄCZENIE APLIKACJI CHOWA WPISY DRUGIEJ, NIE KASUJE ICH. Plik trzyma obie mapy,
          bo porównywanie tych samych instrukcji na dwóch aplikacjach jest zwykłym dniem pracy,
          a utrata ustawień tej, od której się odeszło, jest cicha: człowiek dowiaduje się
          o niej dopiero przy następnym biegu tamtej. */}
      <div className={ROW}>
        <label htmlFor="agent-vendor-options" className={LABEL}>
          {`Extra options for ${appName(value.runsWith)}`}
        </label>
        <textarea
          id="agent-vendor-options"
          data-field="vendorOptions"
          className={FIELD}
          value={textFrom(
            value.vendorOptions?.[PASSTHROUGH_KEY[value.runsWith]] ?? {},
            value.runsWith,
          )}
          placeholder={LIKE[value.runsWith]}
          onChange={(event) =>
            onChange({
              ...value,
              vendorOptions: {
                ...value.vendorOptions,
                [PASSTHROUGH_KEY[value.runsWith]]: pairsFrom(event.target.value, value.runsWith),
              },
            })
          }
        />
        <p className={NOTE}>{`One pair per line, like ${LIKE[value.runsWith]}.`}</p>
      </div>
    </div>
  );
}
