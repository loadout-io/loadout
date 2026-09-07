/* WF-12: jawny wybór w aktywnym projekcie. Otwarcie Settings niczego nie włącza ani nie
 * zapisuje; odpowiedź starego workspace nie może przestawić aktualnie widocznego wyboru.
 *
 * ODCZYT MOŻE ODMÓWIĆ — I WTEDY KARTA MUSI DALEJ DAWAĆ WYBÓR (2026-09-07).
 *
 * CO SIĘ DZIAŁO. Podgląd po stronie Rusta (`inherit::instructions::settings_view`) chodzi po
 * CAŁYM drzewie projektu i wywraca się w całości na pierwszym pliku, którego nie da się
 * otworzyć: dowiązanie `AGENTS.md -> CLAUDE.md` (pliki otwiera `O_NOFOLLOW`), podkatalog bez
 * prawa odczytu, folder projektu zabrany spod okna. Karta pokazywała wtedy odmowę, a POD NIĄ,
 * na zawsze, zdanie „trwa odczyt" — dwa sprzeczne komunikaty naraz i ani jednej kontrolki.
 * Tu stoi jedyny w aplikacji przełącznik projektu i jedyny wybór dla lidera, więc człowiek
 * z włączoną opcją i jednym nieczytelnym plikiem tracił naraz podgląd i wyłącznik: każdy bieg
 * odmawiał startu, a jedyną drogą wyjścia była ręczna edycja `.loadout/project.json`.
 *
 * DLACZEGO NIE „WARTOŚĆ ZASTĘPCZA: WYŁĄCZONE". Ptaszek pinowany na `false` kłamie o pliku,
 * który mówi `true`, a przy okazji umie wysłać WYŁĄCZNIE `true`: React przywraca kontrolowanej
 * kontrolce wartość z propsa po każdym kliknięciu, więc drugie kliknięcie znowu prosiłoby
 * o włączenie i wyłączenie opcji byłoby nieosiągalne. Dlatego po odmowie karta nie udaje, że
 * zna wybór — pokazuje listy BEZ wybranej odpowiedzi, z których każda wartość (w obie strony)
 * jest jednym kliknięciem od zapisu. Zapis wyłączający nie czyta źródeł po stronie Rusta, więc
 * ta droga wychodzi z pułapki naprawdę, a nie tylko na ekranie.
 */
import type { ReactElement } from 'react';
import { useEffect, useRef, useState } from 'react';

import { why } from '../../ipc/why';
import { readProjectSettings, saveProjectSettings } from '../../state/settings-io';
import type { ProjectSettings, ProjectSettingsPatch } from '../../state/settings-io';
import { useWorkspaces } from '../../state/workspaces';

/**
 * Co karta wie o wyborze tego projektu.
 *
 * Trzy stany, nie dwa: „odczyt odmówił" NIE jest „odczyt trwa". Sklejenie ich w jedno `null`
 * było całą treścią wady — zdanie o trwającym odczycie stało pod odmową do końca życia okna.
 */
type ProjectChoice =
  | { readonly kind: 'reading' }
  | { readonly kind: 'unread' }
  | { readonly kind: 'read'; readonly settings: ProjectSettings };

/** Co karta wysyła do Rusta, kiedy człowiek wybierze wartość. */
type Save = (patch: ProjectSettingsPatch) => void;

/**
 * To, co przyjechało, ALBO `null`, jeśli to nie są ustawienia tego projektu.
 *
 * PO CO, zmierzone 2026-09-07. Typ `Promise<ProjectSettings>` w `state/settings-io.ts` jest
 * RZUTOWANIEM (`invoke<ProjectSettings>`), nie sprawdzeniem — granica może oddać cokolwiek,
 * a okno i tak zapisze to jako „przeczytane". Odpowiedź pusta wchodziła więc do stanu, pierwszy
 * render sięgał po `instructions` na pustej wartości i rzucał. Osłona sekcji
 * (`ui/shell/screen-boundary.tsx`) łapie ten rzut i podmienia CAŁY ekran Settings na kartę
 * awarii, więc jedna karta zabierała człowiekowi także wybór lidera, sufit wydatku i przełącznik
 * projektu — a jedyną drogą powrotu był restart aplikacji.
 *
 * SPRAWDZAMY POLA, PO KTÓRE SIĘGA RENDER, a nie „czy to obiekt": kształt niepełny wywala się
 * dokładnie tak samo jak pusty, tylko o jedno pole później.
 */
function settingsFrom(answer: ProjectSettings): ProjectSettings | null {
  const value: unknown = answer;
  if (typeof value !== 'object' || value === null) return null;
  const one = value as Record<string, unknown>;
  const instructions = one['instructions'];
  const limits = one['limits'];
  if (typeof instructions !== 'object' || instructions === null) return null;
  if (typeof (instructions as Record<string, unknown>)['enabled'] !== 'boolean') return null;
  if (!Array.isArray(one['sources'])) return null;
  if (typeof limits !== 'object' || limits === null) return null;
  if (typeof (limits as Record<string, unknown>)['files'] !== 'number') return null;
  return value as ProjectSettings;
}

/** Zdanie dla człowieka, kiedy odpowiedź przyszła, ale nie było w niej ustawień projektu. */
const NOT_SETTINGS = 'Project instructions could not be read. Reopen this project to try again.';

export function ProjectInstructions(): ReactElement | null {
  const folder = useWorkspaces(
    (state) => state.all.find((one) => one.id === state.activeId)?.folder ?? null,
  );
  if (folder === null) return null;
  return <ProjectInstructionsForFolder key={folder} folder={folder} />;
}

function ProjectInstructionsForFolder({ folder }: { readonly folder: string }): ReactElement {
  const [choice, setChoice] = useState<ProjectChoice>({ kind: 'reading' });
  const [said, setSaid] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    void readProjectSettings(folder)
      .then((value) => {
        if (!mounted.current) return;
        const settings = settingsFrom(value);
        if (settings === null) {
          setSaid(NOT_SETTINGS);
          setChoice({ kind: 'unread' });
          return;
        }
        setChoice({ kind: 'read', settings });
      })
      .catch((error: unknown) => {
        if (!mounted.current) return;
        setSaid(why(error, 'Project instructions could not be read.'));
        /* Osobno od zdania odmowy: zdanie może zgasnąć przy następnym zapisie, a fakt „nie
           wiemy, co stoi w pliku" trwa aż do udanego odczytu albo udanego zapisu. */
        setChoice({ kind: 'unread' });
      });
    return () => {
      mounted.current = false;
    };
  }, [folder]);

  async function save(patch: ProjectSettingsPatch): Promise<void> {
    setSaving(true);
    setSaid(null);
    try {
      const value = await saveProjectSettings(folder, patch);
      if (!mounted.current) return;
      /* Ta sama droga po zapisie: potwierdzenie o złym kształcie zabierało ekran tak samo. */
      const settings = settingsFrom(value);
      if (settings === null) {
        setSaid(NOT_SETTINGS);
        setChoice({ kind: 'unread' });
        return;
      }
      setChoice({ kind: 'read', settings });
    } catch (error) {
      if (mounted.current) setSaid(why(error, 'Project instructions could not be saved.'));
    } finally {
      if (mounted.current) setSaving(false);
    }
  }

  const asked: Save = (patch) => {
    void save(patch);
  };

  return (
    <section aria-label="Project instructions" className="card mt-3 max-w-200 stack" data-gap="3">
      <h2 className="text-heading text-ink">Project instructions</h2>
      <p className="caption break-all">{folder}</p>
      {said === null ? null : (
        <p role="alert" className="lead" data-tone="attend">
          {said}
        </p>
      )}
      {choice.kind === 'reading' ? <p className="lead">Reading this project’s settings…</p> : null}
      {choice.kind === 'unread' ? <ChoiceWithoutTheFile saving={saving} asked={asked} /> : null}
      {choice.kind === 'read' ? (
        <ChoiceFromTheFile settings={choice.settings} saving={saving} asked={asked} />
      ) : null}
    </section>
  );
}

/**
 * Wybór, kiedy pliku nie dało się przeczytać.
 *
 * Puste `value` jest tu treścią, nie brakiem: żadna odpowiedź nie jest zaznaczona, bo żadnej
 * nie znamy. Lista, a nie ptaszek, DOKŁADNIE dlatego, że z listy da się wybrać każdą wartość,
 * także tę, którą plik ma dzisiaj — a ptaszek pinowany na jednej wartości umie wysłać tylko
 * tę drugą. Bez tego wyłącznik byłby na ekranie i dalej nie dałoby się wyłączyć.
 */
function ChoiceWithoutTheFile({
  saving,
  asked,
}: {
  readonly saving: boolean;
  readonly asked: Save;
}): ReactElement {
  return (
    <>
      <p className="lead">
        Loadout could not read what this project chose, so nothing below is picked for you. Choosing
        here writes that answer for this project. The list of files, their limits and the choice
        about private files come back as soon as this project’s files can be read again.
      </p>
      <label htmlFor="project-instructions-enabled" className="label block">
        Use project instructions
      </label>
      <select
        id="project-instructions-enabled"
        className="field"
        disabled={saving}
        value=""
        onChange={(event) => {
          asked({ instructions: { enabled: event.target.value === 'on' } });
        }}
      >
        <option value="" disabled>
          Could not be read
        </option>
        <option value="on">Yes</option>
        <option value="off">No</option>
      </select>
      <label htmlFor="lead-project-instructions" className="label block mt-3">
        Lead project instructions
      </label>
      <select
        id="lead-project-instructions"
        className="field"
        disabled={saving}
        value=""
        onChange={(event) => {
          asked({
            leadInstructions: event.target.value === 'inherit' ? null : event.target.value === 'on',
          });
        }}
      >
        <option value="" disabled>
          Could not be read
        </option>
        <option value="inherit">Use project choice</option>
        <option value="on">Always use</option>
        <option value="off">Do not use</option>
      </select>
    </>
  );
}

/** Wybór, kiedy plik odpowiedział: ptaszek pokazuje to, co naprawdę w nim stoi. */
function ChoiceFromTheFile({
  settings,
  saving,
  asked,
}: {
  readonly settings: ProjectSettings;
  readonly saving: boolean;
  readonly asked: Save;
}): ReactElement {
  return (
    <>
      <div className="flex items-center gap-2">
        <input
          id="project-instructions-enabled"
          type="checkbox"
          checked={settings.instructions.enabled}
          disabled={saving}
          onChange={(event) => {
            asked({ instructions: { enabled: event.target.checked } });
          }}
        />
        <label htmlFor="project-instructions-enabled" className="label">
          Use project instructions
        </label>
      </div>
      <p className="lead">
        Applies to new workflows. Each workflow keeps its original instructions; the lead reads the
        current project at the next message. Individual steps and the lead can override this choice.
      </p>
      <details>
        <summary className="label">Sources and scope</summary>
        <p className="lead">
          Text only. Loadout does not grant permissions or import hooks, environment values, or
          agent-app settings through this option. The agent app may separately load its own native
          project instructions; this choice does not disable that behavior.
        </p>
        <p className="caption">
          Up to {settings.limits.files} files, {Math.round(settings.limits.fileBytes / 1024)} KiB
          per file, {Math.round(settings.limits.totalBytes / 1024)} KiB total. A selected file that
          cannot be read is refused, not shortened.
        </p>
        <p className="caption">
          Deeper folders specialize parent folders. At the same scope AGENTS.md takes precedence.
          Rule path patterns retain their scope. Includes use a separate @include relative/file.md
          line; cycles and links are refused.
        </p>
        <ul className="stack" data-gap="1">
          {settings.sources.map((source, index) => (
            <li key={`${source.path}:${index}`} className="caption">
              <span className="break-all">{source.path}</span> — {source.bytes} bytes; folder{' '}
              {source.directory || '.'}
              {source.paths.length === 0 ? null : `; paths ${source.paths.join(', ')}`}
              {source.local ? '; private local source' : null}
            </li>
          ))}
        </ul>
        {settings.sources.length === 0 ? (
          <p className="caption">No project instruction sources found.</p>
        ) : null}
        <div className="flex items-center gap-2 mt-3">
          <input
            id="project-instructions-local"
            type="checkbox"
            checked={settings.instructions.includeLocal}
            disabled={saving}
            onChange={(event) => {
              asked({ instructions: { includeLocal: event.target.checked } });
            }}
          />
          <label htmlFor="project-instructions-local" className="label">
            Include private CLAUDE.local.md files
          </label>
        </div>
        <p className="caption">
          Local files may contain private instructions. Enabling this sends their selected text to
          the chosen agent app.
        </p>
        <label htmlFor="lead-project-instructions" className="label block mt-3">
          Lead project instructions
        </label>
        <select
          id="lead-project-instructions"
          className="field"
          disabled={saving}
          value={
            settings.leadInstructions === null
              ? 'inherit'
              : settings.leadInstructions
                ? 'on'
                : 'off'
          }
          onChange={(event) => {
            asked({
              leadInstructions:
                event.target.value === 'inherit' ? null : event.target.value === 'on',
            });
          }}
        >
          <option value="inherit">Use project choice</option>
          <option value="on">Always use</option>
          <option value="off">Do not use</option>
        </select>
      </details>
    </>
  );
}
