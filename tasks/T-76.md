# T-76 — Agent rozumie niestandardowy setup i proponuje natywny graf

Deterministyczny skan T-75 zostaje pierwszą warstwą dla Claude, Codex, Agent Skills i RuleSync.
Kiedy po nim zostają elementy nierozpoznane, człowiek może jawnie poprosić Claude albo Codex
o analizę odkażonej kopii samych plików setupu. Model zwraca zamknięty draft danych; Rust wiąże
go z hashami źródła, waliduje i dopiero wtedy pokazuje jako agentów oraz workflow do importu.

Analiza niczego nie zapisuje, nie uruchamia skryptów, hooków ani Connections i nie czyta kodu
projektu. Niepokryte zachowania pozostają widoczne jako nierozwiązane. Komenda kroku Check jest
dopuszczona wyłącznie wtedy, gdy stoi dosłownie w wskazanym pliku setupu i ma dowód licznika.
Import ponownie skanuje repo i waliduje oryginalny wynik analizy zamiast ufać webviewowi.

## AC-1 Claude i Codex zwracają tylko zwalidowany draft związany ze źródłem
check: cargo test --test it import_agent_analysis::
expect: (\d+) passed

Fake driver dowodzi read-only `RunSpec`, odkażonego katalogu roboczego, odbioru pełnego JSON,
anulowania z dowodem śmierci i odrzucenia: obcego item id, zmienionego hasha, wymyślonej komendy,
nieistniejącego agenta/skilla oraz grafu, którego natywny walidator nie dopuści do Run.

## AC-2 Prawdziwy modal uruchamia, zatrzymuje i pokazuje analizę przed importem
check: npx --no-install vitest run src/sections/import/analysis-is-real.test.tsx
expect: (\d+) passed

Na ekranie wybiera się Claude/Codex, widać informację o redacted read-only copy, Stop naprawdę
woła osobną komendę, a wynik pokazuje nazwy workflow, kroki i dokładne komendy Check. Import jest
nieaktywny, dopóki coś nadal jest nierozwiązane; Apply niesie analizę i backend sprawdza ją od nowa.

<!-- OWNS
tasks/T-76.md
src-tauri/commands.golden.txt
src-tauri/src/lib.rs
src-tauri/src/ipc.rs
src-tauri/src/commands/import.rs
src-tauri/src/import/mod.rs
src-tauri/src/import/discover.rs
src-tauri/src/import/translate.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/import_agent_analysis.rs
src/sections/import/io.ts
src/sections/import/setup.tsx
src/sections/import/analysis-is-real.test.tsx
-->
