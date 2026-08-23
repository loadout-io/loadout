# T-96 — Sesja agenta mówi prawdę o tym, co dostał

Ekran agenta (`src/sections/run/session/`) ma trzy sekcje: „What X was given", „What X produced",
„What X said". Pierwsza jest kłamstwem z konstrukcji: `AgentScreen` w `session/mount.tsx`
podaje `handoffs: []` i `notes: []` **na sztywno**, a `stepsOf` fabrykuje `brief: ''`
i `files: []`. Sekcja mówi więc „Nothing was given to this agent" w biegu, w którym agent dostał
trzy przekazania i dwie notatki — i to dokładnie w miejscu, które ARCHITECTURE §6b nazywa
panelem „co ten agent dostał" i które ma odpowiadać na pytanie, czy synteza widziała research
(T-87 powstało po tym, jak właściciel **nie mógł tego sprawdzić z ekranu**).

Druga kontrolka: „Run this step again". `Session` renderuje przycisk tylko z `onRunAgain`;
`AgentScreen` podaje go tylko z propsem `onSaid`; jedyne miejsce montażu (`rail/rail.tsx`)
woła `<AgentScreen cards={cards} />` **bez** `onSaid`. Komenda `rerun_step` i cała ścieżka
`again` w Ruście mają wołającego wyłącznie w testach. To jest klasa z niezmiennika 29: funkcja
działa, kiedy ją zawołać — nikt jej nie woła.

Dane są na dysku i na drucie: `list_handoffs(folder)` zwraca `from`, `to`, `title`, `path`,
`bytes` każdego przekazania (`commands/handoffs.rs`), a `read_run` zwraca `memory[]` biegu
(`run.json`, referencje notatek) i `steps[].summary`. Nic nowego w Ruście nie jest potrzebne.

**Read first:** `src/sections/run/session/mount.tsx`, `layout.ts` (`sessionSections`, kształt
`handoffs`/`notes`/`steps`), `session.tsx` (`onRunAgain`) · `src/sections/run/rail/rail.tsx`
(jedyny montaż), `rail/again.ts` (`rerun_step`) · `src/sections/memory/io.ts`
(`list_handoffs`) · `src/sections/run/past/store.ts` (`read_run`) · `src/state/run.ts`
(co `useRun` wie o bieżącym biegu — folder, id) · `tasks/T-87.md` AC-4 (etykiety — tu wolno
je pokazać człowiekowi tym samym zdaniem) · `AGENTS.md` niezmienniki 13, 16, 29.

Słownictwo: „From <agent>", „what it passed on", „Note … — in use". Nie „handoff".

## Kto to robi

- **Agent:** `frontend`
- **Druga opinia:** inny vendor niż pisarz (D3).

## AC-1 „What X was given" listuje prawdziwe przekazania i notatki
check: npx --no-install vitest run src/sections/run/session/given-is-real.test.tsx
expect: (\d+) passed

Dla otwartego agenta sekcja pokazuje wiersz `From <krok>` na każde przekazanie, którego `to`
zawiera ten krok (z `list_handoffs` bieżącego folderu, filtrowane po id biegu z `useRun`),
z tytułem i rozmiarem; oraz wiersz `Note … — in use` na każdą referencję z `memory[]` biegu
(`read_run`). Agent bez poprzedników i bez notatek dostaje dotychczasowe „Nothing was given".
Dane przychodzą przez istniejące adaptery IPC z `vi.mock`; markup przez `renderToStaticMarkup`.
Kontrola: bieg z przekazaniami adresowanymi do **innego** kroku nie pokazuje ich temu agentowi.

## AC-2 „Run this step again" jest na ekranie i woła powtórzenie
check: npx --no-install vitest run src/sections/run/session/run-again-is-reachable.test.tsx
expect: (\d+) passed

`rail.tsx` montuje `AgentScreen` z `onSaid`, więc ekran agenta z `stepId` renderuje przycisk
„Run this step again"; jego handler woła `rerun_step` z plikiem workflow, kafelkiem, `atOnce()`
i folderem z bieżącego biegu, a odpowiedź (zdanie od Rusta) trafia do strumienia przez `onSaid`.
Agent bez `stepId` (pod-agent z biegu) przycisku nie ma. Kryterium renderuje **ścieżkę z `rail.tsx`**,
nie `Session` wprost — inaczej dowodzi tego, co już dziś przechodzi.

<!-- OWNS
tasks/T-96.md
src/sections/run/session/mount.tsx
src/sections/run/session/layout.ts
src/sections/run/session/session.tsx
src/sections/run/session/given-is-real.test.tsx
src/sections/run/session/run-again-is-reachable.test.tsx
src/sections/run/rail/rail.tsx
src/sections/run/rail/again.ts
src/sections/run/index.tsx
-->
