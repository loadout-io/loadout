/* Opis roli → wybrany vendor pisze szkic agenta.
 *
 * # Po co to istnieje
 *
 * Ręczne tworzenie agenta zostaje i niczego nie zastępujemy. Ale formularz agenta ma
 * kilkanaście pól, z których połowa jest rozstrzygnięciem („czym to potwierdzać", „co może
 * zrobić z plikami", „ile myśleć"), a człowiek, który wie, CO ma powstać, nie musi wiedzieć,
 * jak to się nazywa w tym formularzu. Opis plus przycisk jest drogą od jednego do drugiego.
 *
 * # Czego ten wiersz NIE obiecuje
 *
 * Że wynik jest gotowy. Nie ma tu słowa „perfect": to jest **szkic**, otwierany w tym samym
 * edytorze, w którym powstają agenci ręczni, i zapisywany dopiero świadomym kliknięciem.
 * Kliknięcie „Create" uruchamia PISANIE KONFIGURACJI, nie pracę nowego agenta — a to są dwie
 * rzeczy, które z nazwy przycisku łatwo pomylić, więc mówi to zdanie pod polem.
 *
 * # Dwa przyciski zamiast selektora
 *
 * Bo w tej wersji obowiązuje jawna konwencja: pisze ten vendor, dla którego agent powstaje.
 * Selektor „kto pisze" obok „dla kogo" byłby czterema kombinacjami, o które nikt nie prosił,
 * i pytaniem, na które człowiek nie ma jak odpowiedzieć przed pierwszym użyciem.
 */
import { useRef, useState } from 'react';
import type { ReactElement } from 'react';

import type { Agent } from '../../state/agents';
import type { GeneratedDraft } from './io';

export interface GenerateAgentProps {
  /** Prosi wybranego vendora o szkic. Odrzucenie niesie zdanie gotowe na ekran. */
  onGenerate: (
    operation: string,
    described: string,
    runsWith: Agent['runsWith'],
  ) => Promise<GeneratedDraft>;
  /** Zatrzymuje tę jedną operację. */
  onStop: (operation: string) => void;
  /** Oddaje gotowy szkic do istniejącego edytora agenta. */
  onDraft: (draft: GeneratedDraft) => void;
  /** Świeży identyfikator operacji. */
  freshOperation: () => string;
}

/** Stany, które człowiek widzi. Nazwane, bo każdy ma inną kontrolkę pod ręką. */
type Doing =
  | { readonly at: 'idle' }
  | { readonly at: 'generating'; readonly operation: string }
  | { readonly at: 'failed'; readonly said: string }
  | { readonly at: 'cancelled' };

const FIELD = 'field';
const BUTTON = 'btn';

export function GenerateAgent({
  onGenerate,
  onStop,
  onDraft,
  freshOperation,
}: GenerateAgentProps): ReactElement {
  const [described, setDescribed] = useState('');
  const [doing, setDoing] = useState<Doing>({ at: 'idle' });
  /* KTÓRA OPERACJA JEST JESZCZE AKTUALNA — w referencji, nie w stanie.
   *
   * Rozstrzygnięcie „czy ten wynik jest jeszcze na czasie" musi zapaść POZA funkcją
   * aktualizującą stan: React woła ją w trakcie renderu i może zawołać dwa razy, więc
   * skutek uboczny w środku (oddanie szkicu w górę) wykonałby się podwójnie i ustawiałby
   * stan cudzego komponentu w czasie renderowania tego. */
  const current = useRef<string | null>(null);
  const generating = doing.at === 'generating';

  const ask = (runsWith: Agent['runsWith']) => {
    if (described.trim().length === 0) {
      setDoing({ at: 'failed', said: 'Describe what this agent should do first.' });
      return;
    }
    const operation = freshOperation();
    current.current = operation;
    setDoing({ at: 'generating', operation });
    void onGenerate(operation, described, runsWith).then(
      (draft) => {
        /* SPÓŹNIONY WYNIK NIE NADPISUJE NOWSZEGO SZKICU. Odpowiedź operacji, którą człowiek
           już anulował albo zastąpił drugą, jest odpowiedzią na pytanie, które przestało
           obowiązywać — a przyjęta wywracałaby edytor pod ręką piszącego. */
        if (current.current !== operation) return;
        current.current = null;
        setDoing({ at: 'idle' });
        onDraft(draft);
      },
      (error: unknown) => {
        if (current.current !== operation) return;
        current.current = null;
        const said = error instanceof Error ? error.message : String(error);
        setDoing({ at: 'failed', said });
      },
    );
  };

  return (
    <div data-row="generate-agent" className="stack">
      <label htmlFor="agent-description" className="label">
        Describe what this agent should do
      </label>
      <textarea
        id="agent-description"
        className={FIELD}
        rows={3}
        placeholder="Checks that a recording survives Stop then Later, on the running app"
        value={described}
        disabled={generating}
        onChange={(event) => {
          setDescribed(event.target.value);
        }}
      />
      {/* ZDANIE O SKUTKU PRZYCISKU. „Create" czyta się jak „zrób tego agenta i puść go
          w ruch"; to, co się dzieje, jest o krok wcześniej. */}
      <span className="lead">
        This writes the settings for a new agent and opens them here to edit. It does not run the
        agent, and nothing is saved until you press Save.
      </span>

      <div className="flex items-baseline gap-3">
        <button
          type="button"
          className={BUTTON}
          data-field="create-with-codex"
          disabled={generating}
          onClick={() => {
            ask('codex');
          }}
        >
          Create with Codex
        </button>
        <button
          type="button"
          className={BUTTON}
          data-field="create-with-claude"
          disabled={generating}
          onClick={() => {
            ask('claude-code');
          }}
        >
          Create with Claude
        </button>
        {generating ? (
          <button
            type="button"
            className="label text-left hover:text-ink"
            data-field="stop-generating"
            onClick={() => {
              current.current = null;
              onStop(doing.operation);
              setDoing({ at: 'cancelled' });
            }}
          >
            Cancel
          </button>
        ) : null}
      </div>

      {generating ? <span className="lead">Writing the agent…</span> : null}
      {doing.at === 'failed' ? (
        /* ODMOWA STOI PRZY KONTROLCE, a wpisany opis zostaje: człowiek, któremu znika to,
           co napisał, pisze to drugi raz albo rezygnuje. */
        <span className="lead" data-field="generate-problem">
          {doing.said}
        </span>
      ) : null}
      {doing.at === 'cancelled' ? (
        <span className="lead" data-field="generate-cancelled">
          You stopped this. Your description is still here.
        </span>
      ) : null}
    </div>
  );
}

/** Co szkic mówi o sobie — pokazywane obok formularza, zanim człowiek naciśnie Save.
 *
 * # Dwie rzeczy, które łatwo pomylić, i dlatego stoją tu osobno
 *
 * **Poprawna konfiguracja** to wszystko, co dało się sprawdzić kodem: że wskazane
 * umiejętności i połączenia naprawdę istnieją, że ustawienia nie kłócą się z vendorem,
 * że przelotka nie poszerza uprawnień. To jest sprawdzone i widać to niżej.
 *
 * **Sprawdzone zachowanie** to coś zupełnie innego i tego nikt tu nie zrobił. Generator nie
 * jest swoim egzaminatorem: rola napisana przez model, oceniona przez ten sam model, mówi
 * wyłącznie o jakości własnego promptu. Próbę robi Lab, po zapisaniu roli — dlatego to zdanie
 * mówi „nie sprawdzone", zamiast pozwolić, żeby zieleń przy konfiguracji przeczytała się jak
 * zieleń przy działaniu.
 */
export function DraftNotes({ draft }: { draft: GeneratedDraft }): ReactElement | null {
  return (
    <div data-row="draft-notes" className="stack">
      <span className="label">What was checked, and what was not</span>
      <span className="lead" data-field="draft-status">
        The settings below were checked against what this computer actually has. How this agent
        behaves was <strong>not tested</strong>: nothing has run it yet. Save it, then use Evaluate
        to try it on a real case — that is the only thing that can say it works.
      </span>
      {draft.missing.length > 0 ? (
        <>
          <span className="label">What this agent asked for and cannot have here</span>
          <ul className="lead">
            {draft.missing.map((one) => (
              <li key={one}>{one}</li>
            ))}
          </ul>
        </>
      ) : null}
      {draft.refused.length > 0 ? (
        <>
          <span className="label">What was left out of the settings</span>
          <ul className="lead">
            {draft.refused.map((one) => (
              <li key={one}>{one}</li>
            ))}
          </ul>
        </>
      ) : null}
      {draft.assumptions.length > 0 ? (
        <>
          <span className="label">What it had to guess from your description</span>
          <ul className="lead">
            {draft.assumptions.map((one) => (
              <li key={one}>{one}</li>
            ))}
          </ul>
        </>
      ) : null}
      {draft.because.length > 0 ? (
        <>
          <span className="label">Why it chose these settings</span>
          <ul className="lead">
            {draft.because.map((one) => (
              <li key={one}>{one}</li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}
