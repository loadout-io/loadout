# tasks/INDEX.md — widok pochodny

**Ten plik jest GENEROWANY z `tasks/*.md` i `docs/PLAN.md`. Nie edytuj go ręcznie: zmieniasz
plik zadania → regenerujesz ten indeks.** Prawdą jest zadanie, nie ten wiersz. Kolejność to
kolejność budowy z `docs/PLAN.md` §2–§6, nie priorytet.

| ID | Faza | Zadanie | Zależy od | Ścieżek w OWNS | Kryteriów |
|---|---|---|---|---|---|
| **S-1** | 0 | Czy sesja Claude może dostać podzbiór umiejętności? | — | 3 | 2 |
| **S-2** | 0 | Czy --max-turns i --max-budget-usd naprawdę zatrzymują… | — | 3 | 2 |
| **S-3** | 0 | Czy codex exec --json działa end-to-end, i złoty plik z… | — | 4 | 2 |
| **T-01** | 1 | Powłoka aplikacji: okno Tauri, pięć sekcji, tokeny, bez… | — | 12 | 6 |
| **T-02** | 1 | Silnik: graf i planista na FakeDriverze | T-01 | 12 | 7 |
| **T-03** | 1 | Silnik: nadzór procesów i dowód, że grupa nie żyje | T-02 | 7 | 6 |
| **T-04** | 1 | AgentDriver i ClaudeDriver: jeden długo żyjący proces | T-03 | 9 | 7 |
| **T-05** | 1 | Strumień: NDJSON → AgentEvent → Line, plus surowe tee n… | T-04 | 9 | 7 |
| **T-06** | 1 | Magazyn: schemat SQLite, jeden pisarz, migracje, wyzwal… | T-02 | 8 | 7 |
| **T-07** | 1 | IPC: pompa sklejająca 16 ms / 2000 linii i Channel<Vec<… | T-05, T-06 | 8 | 8 |
| **T-25** | 1 | Powłoka montuje sekcje: koniec z pięcioma pustymi ekran… | T-01 | 8 | 5 |
| **T-08** | 1 | Widok pracy: dwie strefy, czternaście rodzajów linii, p… | T-07, T-25 | 5 | 8 |
| **T-09** | 1 | Szyna agentów i widok sesji: „co dostał" i „co wyproduk… | T-08 | 2 | 7 |
| **T-10** | 1 | CodexDriver: pierwszy prawdziwy test, czy AgentDriver j… | T-04, S-3 | 7 | 6 |
| **T-11** | 2 | Definicje agentów: dziewięć pól widocznych, trzy pod „M… | T-01 | 9 | 7 |
| **T-12** | 2 | Format pliku workflow i walidacja w Ruście, przy zapisie | T-02 | 8 | 7 |
| **T-13** | 2 | Płótno: dwa rodzaje kafelka, przeciąganie, znacznik nad… | T-12, T-11, S-1 | 3 | 7 |
| **T-14** | 2 | Lista workflow: utwórz, zduplikuj, usuń | T-12 | 1 | 5 |
| **T-15** | 2 | Uruchom workflow z płótna: domknięcie pętli | T-13, T-08 | 8 | 6 |
| **T-24** | 2 | Workspace'y i karty: kilka folderów naraz, bez utraty… | T-08, T-21 | 3 | 6 |
| **T-16** | 3 | Pliki przekazań: front-matter pisze Loadout, agent daje… | T-05 | 8 | 6 |
| **T-17** | 3 | Sekcja Pamięć: dwa stany, do promptu wchodzi wyłącznie… | T-16, T-06 | 9 | 7 |
| **T-18** | 3 | Umiejętności: silnik rozmieszczania (jeden folder → 2 k… | T-11 | 8 | 6 |
| **T-19** | 3 | Wciąganie umiejętności z linku: nieufna treść, wykrycie… | T-18 | 10 | 8 |
| **T-20** | 4 | Odzyskiwanie po awarii: wykryj, sprzątnij po pgid, zapytaj | T-03, T-06 | 7 | 6 |
| **T-21** | 4 | Limity dostawcy, pauza biegu i suwak „ile naraz" | T-02 | 8 | 8 |
| **T-22** | 4 | Sprawdzacze w bramce: granice modułów, gęstość, testy,… | T-08 | 7 | 7 |
| **T-23** | 4 | Harness Loadouta wyrażony jako workflow Loadouta | T-15, T-13 | 8 | 6 |
| **T-26** | 3 | Cztery sekcje dostają ekran: koniec z kartami, które nic… | T-11, T-13, T-14, T-17, T-19, T-25 | 8 | 4 |

## Jak to czytać

- **Faza 0** to spike'i: wynikiem jest akapit w `docs/research/topics/`, nie kod produkcyjny.
  Dlatego mają po 2 kryteria, a nie 5–8 jak zadania budowlane.
- **Ścieżek w OWNS** — liczba wierszy w bloku `<!-- OWNS -->`. To jedyne źródło własności;
  `checks/quick-scope.sh` czyta ten blok i nic poza nim. Ścieżki **w większości** należą do jednego
  zadania — wyjątkiem są pliki z deklaracjami modułów (`src-tauri/src/lib.rs`, `engine/mod.rs`,
  `memory/mod.rs`, `skills/mod.rs`, `drivers/mod.rs`) i `src/App.tsx`. Każdy z nich jest **wspólnym
  kręgosłupem**: Rust nie wpuści modułu do skrzyni bez `pub mod x;` w rodzicu, więc każde zadanie
  tworzące moduł musi dopisać tam jeden wiersz. `harness/task-spine.py` pilnuje, żeby ten wiersz
  miał gdzie stanąć i żeby proza zadania go nie zabraniała.
- **Kryteriów** — liczba sekcji `## AC-n`. Każda ma dokładnie jedną linię `check:`, a każda
  ścieżka testu jest globalnie unikalna (egzekwuje `harness/gate.py`, `contract_problems`).
- Bramka fazy 1 (`docs/PLAN.md` §3): dwa prawdziwe procesy `claude` **nakładają się w czasie**.
  Dopóki to nie jest udowodnione testem, faza 2 nie zaczyna się.

**T-26 dopisane 2026-08-16, po uruchomieniu aplikacji.** Powłoka i komponenty czterech sekcji były
wylądowane i zielone, a `npm run dev` pokazywał cztery puste ekrany z pięciu: nikt nie napisał
`src/sections/<id>/index.tsx`, bo żadne kryterium o to nie prosiło. `tasks/T-08.md` zakładało, że
pozostałe sekcje dostaną to „za darmo" — nie dostały, a `src/sections/workflows/` nie ma nawet
właściciela w żadnym bloku OWNS.
