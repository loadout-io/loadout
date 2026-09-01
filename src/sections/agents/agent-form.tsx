/* Formularz agenta: SIEDEM wierszy widocznych, i trzy nazwane miejsca, w których stoi reszta.
 *
 * ══ CO SIĘ ZMIENIŁO 2026-08-31 I DLACZEGO ═══════════════════════════════════════════════════
 *
 * Do dziś stało tu dziewięć wierszy plus pięć pod `More settings` — czternaście pól i pięć
 * przycisków, czyli DZIEWIĘTNAŚCIE elementów interaktywnych w kolumnie 332 px. Policzone
 * z tokenów: ~1150 px treści przy 748 px miejsca w oknie, więc `Save` leżał ~400 px pod
 * krawędzią razem ze zdaniem tłumaczącym, czemu jest wygaszony. Nawet ZWINIĘTY formularz
 * zajmował ~720 z 748.
 *
 * Sedno nie jest jednak w pikselach, tylko w randze: JEDENAŚCIE Z CZTERNASTU pól nie wymagało
 * ani jednej decyzji, żeby zapisać działającego agenta — miały działającą wartość domyślną albo
 * pustka była w nich poprawna. Decyzji wymagały trzy: `Name`, `Instructions`, `What it does`.
 * A wszystkie czternaście stały w jednym płaskim stosie, tą samą etykietą, w tym samym rzędzie
 * ważności. Formularz mówił więc, że wszystko jest równie ważne — czyli nie mówił nic.
 *
 * SIEDEM WIERSZY, TRZY GRUPY:
 *   treść agenta          Name, What it does, Instructions
 *   czym myśli            Runs with (jeden wiersz na `Runs with` + `Model` + `Thinking`)
 *   granice               Can it change files, Can it reach the web, Give up after
 *
 * CZEGO TU NIE MA I GDZIE POSZŁO:
 *   `Colour`        — poza formularz w całości. Token przydziela ekran po kolei
 *                     (`index.tsx`), a zmienia się go klikiem w kwadrat na kafelku. Pole było
 *                     dekoracyjne, miało działającą domyślną i wymagało zera decyzji —
 *                     a stało NAD `Instructions`, czyli nad całą treścią agenta.
 *   `Tools`         — zostaje pod `More settings`, gdzie stało. U Codeksa jest niedostępne,
 *                     u Claude'a nie ma ani pickera, ani sprawdzenia wpisu, a jedyna rzecz,
 *                     po którą po nie sięgano — sieć — ma własny wiersz od 2026-08-23.
 *                     CAŁKOWITE USUNIĘCIE TEGO WIERSZA JEST ZGŁOSZONE, NIE ZROBIONE: kryterium
 *                     `src/sections/field-is-a-well-under-its-label.test.tsx` sądzi gałąź pola
 *                     WYŁĄCZONEGO przez vendora, a `Tools` jest jedynym takim polem w całym
 *                     formularzu — plik z tamtym kryterium leży poza zakresem tej zmiany
 *                     i jego własny komentarz mówi wprost, że przy takiej zmianie punkt trzeba
 *                     PRZEKIEROWAĆ, a nie skasować.
 *   `Extra options` — pod osobne, jawne `Advanced`. Surowe argv to nie jest „więcej ustawień",
 *                     tylko inna ranga decyzji: jedna literówka w tym polu zmienia komendę,
 *                     a nie ustawienie.
 *
 * ══ CZEMU FORMULARZ JEST STEROWANY, A ROZWINIĘCIA JUŻ NIE W CAŁOŚCI ══════════════════════════
 *
 * Wartości dalej przychodzą propsem i każda zmiana wychodzi przez `onChange` — powód jest
 * testowy i się nie zmienił: w repo nie ma ani `jsdom`, ani `@testing-library/react`
 * (`package.json` jest na liście DENIED w `checks/quick-scope.sh`), więc formularz sprawdza się
 * przez `renderToStaticMarkup`, a stan trzymany wewnątrz byłby dla takiego renderu niewidoczny.
 *
 * `expanded` (czyli `More settings`) zostaje sterowane z zewnątrz, bo tak jest zadrutowane
 * w `index.tsx` i tak wołają je dwa kryteria spoza tego pliku. Dwa nowe rozwinięcia trzymają
 * stan U SIEBIE, a prop podaje wyłącznie stan POCZĄTKOWY. Powód jest niezmiennikiem 16: prop
 * z handlerem musiałby być OPCJONALNY (kryterium w `src/sections/` renderuje ten formularz bez
 * niego i przestałoby się kompilować), a przycisk z opcjonalnym handlerem jest przyciskiem,
 * który w połowie wywołań nic nie robi. Stan własny daje handler, który istnieje ZAWSZE.
 */
import type { ReactElement } from 'react';
import { useState } from 'react';
import type { Agent, FileAccess, Thinking, Vendor } from '../../state/agents';
import { missingForSave } from '../../state/agents';
import { Advanced } from './advanced';
import { webIsOutOfReach } from './capabilities';
import { MoreSettings } from './more-settings';

export interface AgentFormProps {
  value: Agent;
  /** Czy `More settings` jest rozwinięte. Stan mieszka wyżej — patrz nagłówek pliku. */
  expanded: boolean;
  /** Czy wiersz o tym, czym agent myśli, wstaje rozwinięty. Dalej rozwija go przycisk. */
  brainOpen?: boolean;
  /** Czy `Advanced` wstaje rozwinięte. Ten prop jest szwem dla kryteriów, nie sterowaniem. */
  advancedOpen?: boolean;
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
 * jest podpowiedzią, a nie zamknięciem: pole przyjmuje każdy napis.
 *
 * ALE MÓWI, ŻE PRZYJĘŁO WŁASNY — 2026-08-31. Do dziś „opus4" zapisywało się bez szemrania
 * i padało dopiero w biegu, w środku pracy, na którą ktoś czekał. Zdanie pod polem nie odbiera
 * możliwości wpisania swojego; mówi tylko, że to jest właśnie swoje. */
const MODELS: Record<Vendor, readonly string[]> = {
  'claude-code': ['opus', 'sonnet', 'haiku', 'fable'],
  codex: ['gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna'],
};

/* WYBÓR, A NIE POLE LICZBOWE — 2026-08-31.
 *
 * Stało tu `<input type="number">` z wartością 10, i to była decyzja przebrana za domyślną: dla
 * agenta piszącego kod dziesięć minut jest bardzo mało, a pole liczbowe mówi „każda liczba jest
 * tu w porządku" i nie mówi ani słowa o tym, która jest sensowna. Trzy pozycje odpowiadają na
 * to pytanie wprost — i zero jest wśród nich wartością, nie pustką [T4 §4.3, reguła 1]. */
const GIVE_UP: readonly number[] = [10, 30, 0];

/** „Bez limitu" to zero, nigdy pusta wartość [T4 §4.3, reguła 1]. */
function giveUpSays(minutes: number): string {
  return minutes <= 0 ? 'No limit' : `${String(minutes)} minutes`;
}

/* SIEĆ MA WŁASNY WIERSZ, i od dziś stoi on WŚRÓD WIDOCZNYCH, obok pytania o pliki.
 *
 * 2026-08-23 — z pytania właściciela „czemu dostępu do neta nie mają?". Zmierzone w jego
 * bibliotece: 18 agentów, ani jeden z siecią. U Claude'a dało się ją dostać, WPISUJĄC
 * `WebFetch, WebSearch` w pole `Tools` — i nikt tego nie zrobił, bo nic o tym nie mówi;
 * u Codeksa to pole jest wygaszone, więc nie dało się w ogóle.
 *
 * 2026-08-31 — wiersz przeprowadza się spod `More settings` na wierzch. Powód: to jest pytanie
 * o UPRAWNIENIE, dokładnie tej samej rangi co dial plikowy stojący nad nim, a uprawnienie
 * schowane pod przyciskiem „więcej ustawień" jest uprawnieniem, którego się nie widzi. Te dwa
 * wiersze plus limit czasu są całą granicą, jaką ten agent ma.
 *
 * Zdanie pod przełącznikiem mówi, czego on NIE robi, bo to jest jedyne, o co człowiek pyta
 * w tym miejscu: „czy przez to zacznie mi ruszać pliki". Nie zacznie — dial mówi o plikach,
 * ten przełącznik o świecie. */
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

/* `.field` zostaje pod własną nazwą, bo to nie jest lista klas, tylko jedna nazwa roli — i ta
 * nazwa mieszka w `theme.css` od pierwszego dnia: studnia, mocny obrys, promień z pasma, krój
 * maszynowy i `user-select: text`, bez którego z pola nie da się skopiować własnego wpisu, bo
 * `body` wyłącza zaznaczanie w całej aplikacji. Skupienia tu nie ma z tego samego powodu:
 * `.field:focus` i jeden globalny `:focus-visible` odpowiadają na to raz, dla całej aplikacji. */
const FIELD = 'field';

/* POLE INSTRUKCJI ODDAJE WYSOKOŚĆ DOMOWI, ŻEBY WZIĄĆ JĄ Z LICZBY WIERSZY. 2026-08-31.
 *
 * `textarea.field` w `theme.css` ma `height: 64px` — przy kroju maszynowym 12 px to jakieś 120
 * znaków widocznych naraz, dla CAŁEGO promptu systemowego roli, czyli najdłuższego tekstu
 * w tej aplikacji. `h-auto` jest narzędziem z warstwy `utilities`, więc bije regułę z warstwy
 * `components`, i oddaje wysokość atrybutowi `rows` — czyli liczbie wierszy, którą to pole
 * naprawdę ma. Arkusz zostaje nietknięty (i musi: `src/styles/theme.css` leży poza zakresem
 * tej zmiany), a `resize: vertical` z tej samej reguły dalej pozwala pociągnąć róg.
 *
 * `flex-1` DOSZŁO 2026-08-31, kiedy arkusz roli przestał być kolumną 332 px i wziął całą
 * wysokość ciała ekranu. Wtedy `rows` przestaje być wysokością i staje się WYSOKOŚCIĄ
 * MINIMALNĄ: w kolumnie elastycznej to pole rośnie o całą wysokość, której nie zabrały
 * pozostałe sześć wierszy, a poniżej ośmiu wierszy nie zejdzie, bo minimum elementu
 * elastycznego liczy się z jego treści. `Taller` dalej robi dokładnie to, co mówi — podnosi
 * to minimum ponad to, co zostało, i wtedy arkusz się przewija. */
const AREA = 'field h-auto flex-1';

/* Ile wierszy widać, zanim ktokolwiek o coś poprosi, i ile po naciśnięciu `Taller`.
 *
 * OSIEM, A NIE DWANAŚCIE, I TA LICZBA JEST POLICZONA, NIE WYBRANA. Panel ma 748 px wysokości,
 * z czego 32 px zjada jego własne wypełnienie, a 36 px nagłówek z nazwą agenta — zostaje 680 px.
 * Sześć pozostałych wierszy plus pasek z przyciskami i odstępy formularza (`--spacing` 4 px
 * w wierszu, 12 px między wierszami) to 541 px, więc na pole instrukcji zostaje jakieś 139 px.
 * Osiem wierszy po 18 px plus wypełnienie to 160 px: `Save` stoi wtedy tuż pod krawędzią,
 * a nie 400 px pod nią, jak stał przy czternastu polach.
 *
 * Zysk jest i tak wielokrotny: pole miało 64 px, czyli około 120 znaków całego promptu roli
 * naraz. Osiem wierszy w kolumnie tej szerokości to jakieś 320. Kto pisze więcej, mówi to
 * przyciskiem obok — i wtedy widzi 24 wiersze, bo wtedy pisanie jest tym, co robi. */
const LINES = { some: 8, more: 24 } as const;

/** Wartość z listy albo dotychczasowa. Rzutowanie napisu z DOM-u na wariant enuma byłoby
 * obietnicą, której ten napis nie składa. */
function chosen<T extends string>(options: ReadonlyArray<Choice<T>>, raw: string, now: T): T {
  return options.find((option) => option.value === raw)?.value ?? now;
}

/** Nazwa aplikacji, którą ten agent biegnie — z tabeli, która ją już ma. */
function appName(vendor: Vendor): string {
  return VENDORS.find((one) => one.value === vendor)?.label ?? vendor;
}

/** Pozycje wyboru limitu czasu: trzy nasze plus ta, którą ten agent ma zapisaną na dysku. */
function giveUpChoices(now: number): readonly number[] {
  const mine = Number.isFinite(now) && now > 0 ? now : 0;
  return GIVE_UP.includes(mine) ? GIVE_UP : [...GIVE_UP, mine];
}

/** Liczba minut z pozycji listy. Nieznana pozycja zostawia to, co było. */
function minutesFrom(raw: string, now: number): number {
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : now;
}

export function AgentForm({
  value,
  expanded,
  brainOpen,
  advancedOpen,
  onChange,
  onToggleMore,
  onSave,
}: AgentFormProps): ReactElement {
  const [brain, setBrain] = useState(brainOpen ?? false);
  const [advanced, setAdvanced] = useState(advancedOpen ?? false);
  const [tall, setTall] = useState(false);

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

  /* Model spoza listy jest DOZWOLONY i o tym się mówi — patrz komentarz przy `MODELS`. */
  const typedModel = value.model.trim();
  /* `?? []` — TRZECIA WADA TEJ SAMEJ RODZINY, znaleziona 2026-09-01 przy robieniu zrzutow
       do README. `MODELS` jest `Record<Vendor, …>`, wiec TypeScript uwaza ten odczyt za pewny —
       ale `runsWith` przychodzi Z PLIKU NA DYSKU i nie ma obowiazku byc jednym z dwoch vendorow,
       ktore ta wersja zna. Plik zapisany przez starsza wersje albo poprawiony recznie daje
       `undefined`, a `.includes` na nim przewracalo CALY ekran Agents — tak samo jak wczesniej
       zrobily to `instructions` i `model`. Pusta lista mowi tu prawde: nie znamy modeli tego
       vendora, wiec kazdy wpisany model jest „spoza listy". */
  const ownModel = typedModel !== '' && !(MODELS[value.runsWith] ?? []).includes(typedModel);

  /* Tylko kiedy człowiek o sieć POPROSIŁ: zdanie odbierające coś, czego nikt nie chciał,
   * jest szumem, a szum uczy przewijać wzrokiem każdą uwagę w tym formularzu. */
  const webWontReach = value.reachesTheWeb && webIsOutOfReach(value.runsWith, value.fileAccess);

  return (
    <form
      data-agent-form
      /* `flex-1`, bo od 2026-08-31 formularz stoi w kolumnie o wysokości ciała ekranu, a nie
         w rurze 332 px: bez tego wiersz instrukcji nie miałby czego dzielić i pole wracałoby
         do ośmiu wierszy pod półmetrem pustki. `.stack` jest już kolumną elastyczną. */
      className="stack flex-1"
      data-gap="3"
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
      <div className="stack">
        <label htmlFor="agent-name" className="label">
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

      <div className="stack">
        <label htmlFor="agent-summary" className="label">
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

      {/* INSTRUKCJE STOJĄ TRZECIE I DOSTAJĄ NAJWIĘCEJ MIEJSCA W CAŁYM FORMULARZU, bo są całą
          treścią agenta. Do 2026-08-31 stały czwarte, pod `Colour`. */}
      {/* `flex-1` NA WIERSZU INSTRUKCJI — 2026-08-31. To jest ten jeden wiersz formularza,
          któremu wolno urosnąć o całą wolną wysokość arkusza: instrukcje są całą treścią roli,
          a pozostałych sześć wierszy to jedna kontrolka każdy i wyższe być nie mają jak. */}
      <div className="stack flex-1">
        <div className="flex items-center gap-2">
          <label htmlFor="agent-instructions" className="label">
            Instructions
          </label>
          {/* Uchwyt do ciągnięcia w rogu pola JEST (`resize: vertical` w arkuszu) i nikt go nie
              znajduje. Ten przycisk mówi to samo słowem. */}
          <button
            type="button"
            data-taller
            className="btn-bare ml-auto"
            onClick={() => {
              setTall((was) => !was);
            }}
          >
            {tall ? 'Shorter' : 'Taller'}
          </button>
        </div>
        <textarea
          id="agent-instructions"
          data-field="instructions"
          className={AREA}
          rows={tall ? LINES.more : LINES.some}
          aria-required="true"
          value={value.instructions}
          onChange={(event) => onChange({ ...value, instructions: event.target.value })}
        />
      </div>

      {/* JEDNO PYTANIE, NIE TRZY — 2026-08-31.
       *
       * `Runs with`, `Model` i `Thinking` to trzy kontrolki na jedno pytanie („czym ten agent
       * myśli"), wszystkie trzy z działającą domyślną. Zwinięty wiersz czyta całą odpowiedź
       * naraz, więc człowiek widzi, co dostanie, i nie musi jej podawać. */}
      {brain ? (
        <>
          <div className="stack">
            <div className="flex items-center gap-2">
              <label htmlFor="agent-runs-with" className="label">
                Runs with
              </label>
              <button
                type="button"
                data-brain
                aria-expanded="true"
                className="btn-bare ml-auto"
                onClick={() => {
                  setBrain(false);
                }}
              >
                Done
              </button>
            </div>
            <select
              id="agent-runs-with"
              data-field="runsWith"
              className={FIELD}
              value={value.runsWith}
              onChange={(event) =>
                onChange({
                  ...value,
                  runsWith: chosen(VENDORS, event.target.value, value.runsWith),
                })
              }
            >
              {VENDORS.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>

          <div className="stack">
            <label htmlFor="agent-model" className="label">
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
            {ownModel ? (
              <p data-own-model className="lead">
                {`${typedModel} is your own — ${appName(value.runsWith)} gets it exactly as typed.`}
              </p>
            ) : null}
          </div>

          <div className="stack">
            <label htmlFor="agent-thinking" className="label">
              Thinking
            </label>
            <select
              id="agent-thinking"
              data-field="thinking"
              className={FIELD}
              value={value.thinking}
              onChange={(event) =>
                onChange({
                  ...value,
                  thinking: chosen(THINKING, event.target.value, value.thinking),
                })
              }
            >
              {THINKING.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </div>
        </>
      ) : (
        <div className="stack">
          {/* NAZWA WIERSZA W `<span>`, NIE W `<label>`, i to jest wymuszone, nie wybrane:
              zwinięty wiersz nie ma kontrolki formularza, tylko przycisk, a `<label for>`
              wskazujący na przycisk jest etykietą wskazującą na coś, co etykiety nie przyjmuje.
              Ranga napisu zostaje ta sama, bo niesie ją klasa. */}
          <span className="label">Runs with</span>
          <button
            type="button"
            data-brain
            aria-expanded="false"
            className="row"
            onClick={() => {
              setBrain(true);
            }}
          >
            <span data-brain-says>
              {`${appName(value.runsWith)} · ${value.model} · ${
                THINKING.find((one) => one.value === value.thinking)?.label ?? value.thinking
              }`}
            </span>
            <span className="value ml-auto">Change</span>
          </button>
        </div>
      )}

      <div className="stack">
        <label htmlFor="agent-file-access" className="label">
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

      <div className="stack">
        <label htmlFor="agent-web" className="label">
          Can it reach the web
        </label>
        {/* LISTA WYBORU, nie przełącznik, i nie z upodobania: wiersz obok — dial dostępu do
            plików — jest `<select>` z klasą domu, a dwa pytania o uprawnienia, zadane dwiema
            różnymi kontrolkami, czytają się jak dwie różne rangi decyzji. Jeden kształt na
            jedną robotę, we wszystkich pięciu sekcjach. */}
        <select
          id="agent-web"
          data-field="reachesTheWeb"
          className={FIELD}
          value={value.reachesTheWeb ? 'yes' : 'no'}
          onChange={(event) => {
            onChange({ ...value, reachesTheWeb: event.target.value === 'yes' });
          }}
        >
          <option value="no">No</option>
          <option value="yes">Read and search the web</option>
        </select>
        <p className="lead">{WEB_IS_NOT_ABOUT_FILES}</p>
        {webWontReach ? <p className="lead">{WEB_NEEDS_WRITE_ACCESS}</p> : null}
      </div>

      <div className="stack">
        <label htmlFor="agent-give-up-after" className="label">
          Give up after
        </label>
        <select
          id="agent-give-up-after"
          data-field="giveUpAfterMinutes"
          className={FIELD}
          value={String(value.giveUpAfterMinutes)}
          onChange={(event) =>
            onChange({
              ...value,
              giveUpAfterMinutes: minutesFrom(event.target.value, value.giveUpAfterMinutes),
            })
          }
        >
          {giveUpChoices(value.giveUpAfterMinutes).map((minutes) => (
            <option key={minutes} value={String(minutes)}>
              {giveUpSays(minutes)}
            </option>
          ))}
        </select>
      </div>

      {expanded ? <MoreSettings value={value} onChange={onChange} /> : null}
      {advanced ? <Advanced value={value} onChange={onChange} /> : null}

      <div className="stack border-t border-line pt-3" data-gap="2">
        <div className="flex items-center gap-2">
          <button
            type="button"
            data-more
            aria-expanded={expanded}
            className="btn-quiet"
            onClick={onToggleMore}
          >
            More settings
          </button>
          {/* DWA PRZYCISKI, NIE JEDEN, i to jest cała treść piątego punktu tej zmiany: surowe
              argv nie są „więcej ustawień". Jeden przycisk na oba znaczy człowieka, który
              otwiera jedno i znajduje drugie. */}
          <button
            type="button"
            data-advanced
            aria-expanded={advanced}
            className="btn-quiet"
            onClick={() => {
              setAdvanced((was) => !was);
            }}
          >
            Advanced
          </button>
          <button
            type="submit"
            data-save
            disabled={!saveable}
            /* Powód jest PODPISANY pod przyciskiem, nie tylko na nim: `aria-describedby`
             * wiąże wygaszony przycisk ze zdaniem, więc czytnik ekranu mówi jedno i drugie
             * w jednym oddechu, zamiast „Save, niedostępny" i ciszy. */
            aria-describedby={saveable ? undefined : 'agent-save-blocked'}
            className="btn-primary ml-auto"
          >
            Save
          </button>
        </div>
        {missing === null ? null : (
          <p id="agent-save-blocked" data-save-blocked className="lead">
            {missing}
          </p>
        )}
      </div>
    </form>
  );
}
