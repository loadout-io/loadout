# WF-13 — natywne dostarczanie skilli

Stan lokalnego rozpoznania: 2026-09-06, worktree `loadout-workflow-reliability-build`,
baza `715567effb059ec3e684de2ba17d231b854d0b04` i niezatwierdzone zmiany WF.
To nie jest dowód ukończenia karty ani żywy test użycia skilla przez model.

## Wersje i ograniczony odczyt

- `codex-cli 0.153.4`: `codex exec --help`, `codex plugin --help`,
  `codex debug --help` i `codex app-server generate-json-schema --experimental`.
- `Claude Code 2.1.261`: istniejący produkcyjny adapter i lokalna pomoc CLI.
- Bez uruchomienia płatnej sesji, globalnego instalowania pluginów i dopisywania
  czegokolwiek do półek użytkownika.

## Cztery sesje

| Sesja | Potwierdzony natywny mechanizm | Granica obecnego dowodu |
|---|---|---|
| Claude, krok | `--plugin-dir` wskazuje prywatny plugin z pełnymi katalogami `skills/<name>` | Runtime z dublerem sprawdza osiągalne reference/helper i zachowany bit wykonywalności; żywy model jeszcze niezweryfikowany na finalnym SHA. |
| Claude, Lead | Ten sam natywny plugin CLI | Po RED test prawdziwego Lead IPC z dublerem potwierdza oddzielne niezmienione paczki dwóch rozmów i powiązanie każdej z historią. |
| Codex, krok | Natywna półka `.agents/skills` we własnej świeżej kopii kroku | Prawdziwy `CodexDriver` z procesem-dublerem potwierdza pełne zamrożone pliki. Brak potwierdzonego per-invocation prywatnego katalogu pluginu dla `codex exec`: praca wprost w repo odmawia przed spawn. Borrow do własnej kopii przeszedł właściwy RED i GREEN tą samą natywną drogą. |
| Codex, Lead | `turn/start.params.input`: `{ "type": "skill", "name": "…", "path": "…/SKILL.md" }` | Lokalny `v2/TurnStartParams.json` potwierdza typ i trzy wymagane pola; po RED prawdziwa droga IPC i `CodexDriver` wysyłają to wejście do procesu-dublera, ze wskazaniem pełnej prywatnej paczki. To nie zastępuje żywego odczytu zasobu przez model. |

## Czego rozpoznanie nie uprawnia robić

Aktualna dokumentacja opisuje `skills/list.perCwdExtraUserRoots`, ale wygenerowany schemat
lokalnego CLI 0.153.4 zawiera tam tylko `cwds` i `forceReload`. Nie wysyłamy niepotwierdzonego
pola, zakładając, że musiało już trafić do zainstalowanej wersji.

`skills.config` w konfiguracji oznacza per-skill enablement overrides, nie potwierdza
dodatkowego korzenia odkrywania. `--add-dir` poszerza zapisywalny obszar i nie służy do
dostarczenia skilla agentowi z węższą polityką. `codex plugin add` zmienia instalację
użytkownika i nie jest substytutem prywatnej paczki jednej sesji.

Ostrzeżenie i kontynuacja rozmowy bez wsparcia pozostają uczciwą degradacją, nie stanem
`Delivered`. Osiągalne pliki nie dowodzą, że model faktycznie ich użył. Cztery działające
sesje oraz żywy odczyt losowego markera przez oba vendory na finalnym SHA nadal są
warunkami zamknięcia WF-13.

Pochodzenie natywnych plików zapisuje teraz `nativeSkills` w istniejącym znaczniku kopii.
Cleanup wymaga niezmienionych tożsamości i treści oraz potwierdzonego końca wszystkich
użytkowników kopii. Kolejny krok odtwarza wybrane skille z zamrożonej paczki. Nie powstał
globalny ignore `.agents`: usunięto wcześniejsze zbiorcze wykluczenie `.agents/skills`
z zapisu Git, bo gubiło pracę człowieka. Trzy testy runtime najpierw odtworzyły błędy,
a po poprawce potwierdziły brak generated package w fan-in i zachowanym folderze oraz
zapis pre-existing i nowych autorskich skilli w dokładnym commicie wyniku.

Ostatni potwierdzony wynik lokalny tego zakresu: moduł WF-13 **18/18** (3,10 s),
starsze kryterium Lead **4/4** (0,17 s). Wynik obejmuje także zmianę inode przy tych samych
bajtach, zmianę treści oraz ponowne dostarczenie w tej samej kopii. Granica dotyczy wyników
publikowanych przez Loadout; nie jest
ogólną blokadą arbitralnych komend Git wykonywanych przez agenta z prawem do powłoki.

Źródła pomocnicze (lokalny schemat ma pierwszeństwo przy stwierdzeniu dostępności):
[App Server](https://learn.chatgpt.com/docs/app-server),
[budowa skilli](https://learn.chatgpt.com/docs/build-skills),
[konfiguracja](https://learn.chatgpt.com/docs/config-file/config-reference).
