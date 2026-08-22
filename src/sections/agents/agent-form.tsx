/* Formularz agenta: dziewięć wierszy i przycisk, który rozwija trzy.
 *
 * Formularz jest STEROWANY: wartości i stan rozwinięcia przychodzą propsami, a każda zmiana
 * wychodzi przez `onChange`. Powód nie jest architektoniczny, tylko testowy — w repo nie ma
 * `jsdom` ani `@testing-library/react` (`package.json` jest na liście DENIED w
 * `checks/quick-scope.sh`), więc formularz sprawdzamy przez `renderToStaticMarkup`. Statyczny
 * HTML wystarcza na kolejność etykiet i na atrybut `disabled`, a stan trzymany wewnątrz
 * komponentu byłby dla takiego testu niewidoczny.
 *
 * Dziewięć wierszy jest wiążące: `docs/mockup/index.html`, panel `Forge` w sekcji Agents.
 * Pole wchodzi tu tylko wtedy, gdy zauważyłbyś jego brak w pierwszej godzinie [T4 §3].
 *
 * Trzy schowane wiersze są POZA drzewem, a nie schowane stylem. Kontrolka pod `display:none`
 * dalej jest w HTML, dalej ma etykietę, dalej rośnie — i na zrzucie ekranu wygląda identycznie
 * jak wersja poprawna. „Co jest otwarte" rozstrzyga się tutaj, w TypeScripcie, nie w arkuszu
 * stylów (niezmiennik 15).
 *
 * Czego tu NIE MA, choć T4 §8.1 to rysuje: wiersza `Write results to`. Makieta ma w tym
 * miejscu `Colour` i nie ma ścieżki wyniku (`docs/mockup/index.html:624-632`), a makieta jest
 * zatwierdzona. `writeResultsTo` zostaje w typie z domyślnym `""` i jest ustawiane NA KROKU
 * (`docs/mockup/index.html:559`), bo ścieżka wyniku należy do kroku, nie do roli.
 */
import type { ReactElement } from 'react';
import type { Agent, Color, FileAccess, Thinking, Vendor } from '../../state/agents';
import { missingForSave } from '../../state/agents';
import { MoreSettings } from './more-settings';

export interface AgentFormProps {
  value: Agent;
  /** Czy `More settings` jest rozwinięte. Stan mieszka wyżej — patrz nagłówek pliku. */
  expanded: boolean;
  onChange: (next: Agent) => void;
  onToggleMore: () => void;
  onSave: () => void;
}

interface Choice<T extends string> {
  value: T;
  label: string;
}

/* Brzmienia z tabeli „We say / We never say" [T4 §8.1] i z makiety. Żadna z tych etykiet nie
 * jest nazwą z drutu: `look-only` nigdy nie dociera na ekran (niezmiennik 14). */
const COLOURS: ReadonlyArray<Choice<Color>> = [
  { value: 'slate', label: 'Slate' },
  { value: 'plum', label: 'Plum' },
  { value: 'clay', label: 'Clay' },
  { value: 'moss', label: 'Moss' },
  { value: 'rose', label: 'Rose' },
];

/* Eksportowane, bo `index.tsx` potrzebowało DOKŁADNIE tych brzmień na kafelku i do 2026-08-18
 * trzymało własną kopię tej tabeli (jej nagłówek nazywał to długiem i prosił o tę jedną linię).
 * Dwie kopie brzmienia rozjeżdżają się przy pierwszej zmianie i nikt się o tym nie dowie:
 * nazwa z drutu (`claude-code`) nie ma prawa dojechać na ekran (niezmiennik 14), więc obie
 * kopie i tak wyglądają na poprawne. */
export const VENDORS: ReadonlyArray<Choice<Vendor>> = [
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'codex', label: 'Codex' },
];

const THINKING: ReadonlyArray<Choice<Thinking>> = [
  { value: 'quick', label: 'Quick' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'deep', label: 'Deep' },
  { value: 'deepest', label: 'Deepest' },
];

const FILE_ACCESS: ReadonlyArray<Choice<FileAccess>> = [
  { value: 'look-only', label: 'Look only' },
  { value: 'ask-first', label: 'Ask first' },
  { value: 'work-freely', label: 'Work freely' },
];

/* Udokumentowane aliasy plus wolny tekst — dlatego `<input list>`, a nie `<select>`. Prawdziwą
 * listę modeli daje CLI (`codex debug models` zwraca katalog z `visibility`, T4 §6.4), a to
 * wchodzi razem ze sterownikami (T-04, T-10). Zaszyte slugi rdzewieją w tygodnie, więc ta lista
 * jest podpowiedzią, a nie zamknięciem: pole przyjmuje każdy napis. */
const MODELS: Record<Vendor, readonly string[]> = {
  'claude-code': ['opus', 'sonnet', 'haiku', 'fable'],
  codex: ['gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna'],
};

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
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
const AREA = 'field';

/* Klasa przycisku Save zależy od stanu i jest wybierana TUTAJ, a nie wariantem `disabled:`
 * Tailwinda. Wariant zostawiłby słowo `disabled` w atrybucie `class` także wtedy, gdy przycisk
 * działa — czyli „czy da się zapisać" miałoby w HTML-u dwie odpowiedzi, z których jedna kłamie
 * (niezmiennik 13: jeden fakt, jedno miejsce). */
const SAVE = 'ml-auto h-8 rounded-sm bg-accent px-4 text-ui text-bg';
const SAVE_OFF = 'ml-auto h-8 rounded-sm bg-raised px-4 text-ui text-muted';

/** Wartość z listy albo dotychczasowa. Rzutowanie napisu z DOM-u na wariant enuma byłoby
 * obietnicą, której ten napis nie składa. */
function chosen<T extends string>(options: ReadonlyArray<Choice<T>>, raw: string, now: T): T {
  return options.find((option) => option.value === raw)?.value ?? now;
}

/** „Bez limitu" to zero, nigdy pusta wartość [T4 §4.3, reguła 1]. */
function minutesFrom(raw: string): number {
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

export function AgentForm({
  value,
  expanded,
  onChange,
  onToggleMore,
  onSave,
}: AgentFormProps): ReactElement {
  /* Nazwa i instrukcje. Reszta ma wartość domyślną, więc Save budzi się dokładnie wtedy, gdy
   * te dwa pola są wypełnione [T4 §8.1]. Agent bez instrukcji to nazwa.
   *
   * Jedno pytanie, jedna odpowiedź, i mieszka ONA W MAGAZYNIE (`missingForSave`
   * w `src/state/agents.ts`) — nie tutaj. Powód jest mechaniczny: `store.save` odmawia po tym
   * samym warunku, bo jest jedyną krawędzią do dysku, a dwie kopie reguły znaczą przycisk,
   * który budzi się przy trzecim polu wymaganym, i zapis, który dalej go nie przyjmuje
   * (niezmiennik 13). */
  const missing = missingForSave(value);
  const saveable = missing === null;

  return (
    <form
      data-agent-form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        /* Wygaszony przycisk NIE JEST całą obroną. Formularz z jednym polem tekstowym wysyła
         * się też Enterem, a zachowanie przeglądarki przy wygaszonym przycisku domyślnym nie
         * jest jednakowe wszędzie — i to jest dokładnie ta droga, którą do magazynu jechałby
         * agent bez instrukcji, czyli plik, który walidator biegu odrzuci pod palcem. */
        if (!saveable) return;
        onSave();
      }}
    >
      <div className={ROW}>
        <label htmlFor="agent-name" className={LABEL}>
          Name
        </label>
        <input
          id="agent-name"
          data-field="name"
          className={FIELD}
          /* `aria-required`, a nie `required`: walidacja HTML-a wyświetla własny balonik
           * przeglądarki, którego brzmienia nie kontrolujemy i który mówi „Please fill out
           * this field" obok naszego zdania. Powód stoi pod przyciskiem, jeden raz. */
          aria-required="true"
          value={value.name}
          onChange={(event) => onChange({ ...value, name: event.target.value })}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="agent-summary" className={LABEL}>
          What it does
        </label>
        <input
          id="agent-summary"
          data-field="summary"
          className={FIELD}
          value={value.summary}
          onChange={(event) => onChange({ ...value, summary: event.target.value })}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="agent-color" className={LABEL}>
          Colour
        </label>
        <select
          id="agent-color"
          data-field="color"
          className={FIELD}
          value={value.color}
          onChange={(event) =>
            onChange({ ...value, color: chosen(COLOURS, event.target.value, value.color) })
          }
        >
          {COLOURS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <div className={ROW}>
        <label htmlFor="agent-instructions" className={LABEL}>
          Instructions
        </label>
        <textarea
          id="agent-instructions"
          data-field="instructions"
          className={AREA}
          aria-required="true"
          value={value.instructions}
          onChange={(event) => onChange({ ...value, instructions: event.target.value })}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="agent-runs-with" className={LABEL}>
          Runs with
        </label>
        <select
          id="agent-runs-with"
          data-field="runsWith"
          className={FIELD}
          value={value.runsWith}
          onChange={(event) =>
            onChange({ ...value, runsWith: chosen(VENDORS, event.target.value, value.runsWith) })
          }
        >
          {VENDORS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <div className={ROW}>
        <label htmlFor="agent-model" className={LABEL}>
          Model
        </label>
        <input
          id="agent-model"
          data-field="model"
          className={FIELD}
          list="agent-model-choices"
          value={value.model}
          onChange={(event) => onChange({ ...value, model: event.target.value })}
        />
        <datalist id="agent-model-choices">
          {MODELS[value.runsWith].map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
      </div>

      <div className={ROW}>
        <label htmlFor="agent-thinking" className={LABEL}>
          Thinking
        </label>
        <select
          id="agent-thinking"
          data-field="thinking"
          className={FIELD}
          value={value.thinking}
          onChange={(event) =>
            onChange({ ...value, thinking: chosen(THINKING, event.target.value, value.thinking) })
          }
        >
          {THINKING.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <div className={ROW}>
        <label htmlFor="agent-file-access" className={LABEL}>
          Can it change files
        </label>
        <select
          id="agent-file-access"
          data-field="fileAccess"
          className={FIELD}
          value={value.fileAccess}
          onChange={(event) =>
            onChange({
              ...value,
              fileAccess: chosen(FILE_ACCESS, event.target.value, value.fileAccess),
            })
          }
        >
          {FILE_ACCESS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </div>

      <div className={ROW}>
        <label htmlFor="agent-give-up-after" className={LABEL}>
          Give up after
        </label>
        <div className="flex items-center gap-2">
          <input
            id="agent-give-up-after"
            data-field="giveUpAfterMinutes"
            className={FIELD}
            type="number"
            min={0}
            value={String(value.giveUpAfterMinutes)}
            onChange={(event) =>
              onChange({ ...value, giveUpAfterMinutes: minutesFrom(event.target.value) })
            }
          />
          <span className="text-body text-muted">minutes</span>
        </div>
      </div>

      {expanded ? <MoreSettings value={value} onChange={onChange} /> : null}

      <div className="flex flex-col gap-2 border-t border-line pt-3">
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-more
            aria-expanded={expanded}
            className="h-8 rounded-sm border border-line px-3 text-ui text-body"
            onClick={onToggleMore}
          >
            More settings — tools, skills, connections
          </button>
          <button
            type="submit"
            data-save
            disabled={!saveable}
            /* Powód jest PODPISANY pod przyciskiem, nie tylko na nim: `aria-describedby`
             * wiąże wygaszony przycisk ze zdaniem, więc czytnik ekranu mówi jedno i drugie
             * w jednym oddechu, zamiast „Save, niedostępny" i ciszy. */
            aria-describedby={saveable ? undefined : 'agent-save-blocked'}
            className={saveable ? SAVE : SAVE_OFF}
          >
            Save
          </button>
        </div>
        {missing === null ? null : (
          <p id="agent-save-blocked" data-save-blocked className="text-body text-muted">
            {missing}
          </p>
        )}
      </div>
    </form>
  );
}
