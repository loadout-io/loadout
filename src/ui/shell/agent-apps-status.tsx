/* Stopka bocznego menu: co naprawdę odpowiedziało, w dwóch trybach tej samej nawigacji.
 *
 * ZDANIE, NIE STAŁA (2026-09, Z-34). Do tego dnia stało tu `READY = 'Claude · Codex ready'`
 * i własny komentarz przy tej stałej przyznawał, dlaczego: „`commands.golden.txt` nie ma dziś
 * ani jednej komendy, która pyta o stan dostawców". `check_agent_apps` jest tą komendą, więc
 * napis zniknął razem z zaszytą wiedzą o vendorach.
 *
 * TRYB ZWINIĘTY ZOSTAWIA SAMĄ KROPKĘ. Reguła tej kolumny jest jedna i stoi w nagłówku
 * `titlebar.tsx`: rzecz znika tylko wtedy, kiedy jej brak NIE KŁAMIE. Lista aplikacji ucięta do
 * dwóch znaków obiecywałaby gotowość kogoś, kogo na niej nie widać, więc zdania jadą w całości
 * do podpowiedzi i do nazwy dostępnej — a jedna kropka niesie trzy stany dwóch aplikacji, bo
 * jeden fakt ma jedno miejsce (niezmiennik 13).
 *
 * NIE JEST TO BRAMA STARTU ani druga wyrocznia. Migawka mówi, co widziała; odmowę uruchomienia
 * wydaje `check_to_run` po stronie Rusta, jak dotąd.
 */
import type { ReactElement } from 'react';
import { useSyncExternalStore } from 'react';

import type { AgentAppStatus } from '../../state/agent-apps';
import { useAgentApps } from '../../state/agent-apps';

/** Nazwy własne obu aplikacji, po angielsku (decyzja D5) i tak, jak nazywa je ich dom. */
type AppName = 'Claude Code' | 'Codex';

/**
 * Zdanie o jednej aplikacji, którego brak nie kłamie w żadnym z czterech stanów.
 *
 * „Sprawdzamy" jest osobnym zdaniem od „nie ma", bo pierwsze mówi o nas, a drugie o świecie:
 * okno otwarte na maszynie z obiema aplikacjami pokazywałoby inaczej przez ułamek sekundy —
 * a przy odmowie granicy na zawsze — że nie ma niczego.
 */
function sentence(app: AppName, status: AgentAppStatus): string {
  switch (status.state) {
    case 'checking':
      return `Checking ${app}…`;
    case 'found':
      return `${app} · ${status.version}`;
    case 'not-found':
      return `${app} wasn't found.`;
    case 'could-not-check':
      return `Loadout couldn't check ${app}.`;
  }
}

/**
 * WERSJA TO NIE JEST ZALOGOWANIE, i to zdanie stoi tu po to, żeby nikt nie musiał tego zgadywać.
 * `--version` odpowiada tak samo wylogowanemu i zalogowanemu; jedyny znany sygnał o logowaniu
 * przychodzi z prawdziwej, płatnej tury [T1 §3.3]. Znaleziona wersja bez tego zdania czytałaby
 * się jak „gotowe", czyli byłaby tą samą obietnicą, którą ta stopka właśnie przestała składać.
 */
const SIGN_IN = 'Sign-in is checked when you first run an agent.';

/** Czy jest jeszcze o co pytać. Dwie znalezione aplikacje nie zostawiają Retry bez pracy. */
function worthRetrying(statuses: readonly AgentAppStatus[]): boolean {
  return statuses.some(
    (status) => status.state === 'not-found' || status.state === 'could-not-check',
  );
}

/**
 * Kropka gotowości: PRZYGASZONA i STOJĄCA, w obu trybach ta sama.
 *
 * Akcent w tym systemie znaczy „to jest interaktywne", a barwa żywa znaczy „to się dzieje
 * teraz" — dostępność lokalnej aplikacji nie jest ani jednym, ani drugim (DESIGN §3). Nie
 * pulsuje z drugiego, policzonego powodu: dwa regiony ruchu z `docs/ARCHITECTURE.md` §7 są już
 * wydane (`sections/run/graph/tile.tsx`, `sections/run/tabs/tab.tsx`).
 */
const DOT = 'size-[7px] shrink-0 rounded-full bg-muted';

export interface AgentAppsStatusProps {
  /** Czy boczne menu stoi zwinięte do samych ikon. Przychodzi propsem — patrz `titlebar.tsx`. */
  readonly collapsed: boolean;
}

/** Jedyne żywe miejsce, w którym okno mówi, co odpowiedziały lokalne aplikacje agentów. */
export function AgentAppsStatus({ collapsed }: AgentAppsStatusProps): ReactElement {
  /* `getState` w OBU migawkach, nie hak zustanda, i powód jest ten sam, co w `titlebar.tsx`:
   * `renderToStaticMarkup` bierze migawkę SERWEROWĄ, a ta u zustanda jest stanem z chwili
   * utworzenia magazynu — komponent czytający hakiem byłby w każdym kryterium pusty. */
  const apps = useSyncExternalStore(
    useAgentApps.subscribe,
    useAgentApps.getState,
    useAgentApps.getState,
  );
  const said = [sentence('Claude Code', apps.claudeCode), sentence('Codex', apps.codex), SIGN_IN];

  if (collapsed) {
    return (
      <span
        data-agent-apps-status
        role="img"
        title={said.join(' · ')}
        aria-label={said.join(' · ')}
        className={DOT}
      />
    );
  }

  return (
    <div data-agent-apps-status className="flex min-w-0 flex-col gap-[6px]">
      {[['Claude Code', apps.claudeCode] as const, ['Codex', apps.codex] as const].map(
        ([app, status]) => (
          <span key={app} className="flex items-start gap-[7px]">
            <span aria-hidden className={'mt-[3px] ' + DOT} />
            <span className="min-w-0 truncate">{sentence(app, status)}</span>
          </span>
        ),
      )}
      <span>{SIGN_IN}</span>
      {worthRetrying([apps.claudeCode, apps.codex]) ? (
        <button
          type="button"
          data-agent-apps-retry
          onClick={() => {
            /* Odpowiedź jest ZGUBIONA ŚWIADOMIE: magazyn sam zapisuje w sobie każdy z czterech
               stanów, więc nie ma tu czego jeszcze przeczytać. Nakładające się kliknięcie
               dostaje trwający odczyt, nie drugą parę procesów (`state/agent-apps.ts`). */
            void useAgentApps.getState().check();
          }}
          className="w-fit text-accent transition-colors hover:text-ink"
        >
          Retry
        </button>
      ) : null}
    </div>
  );
}
