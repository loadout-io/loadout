# Loadout

Aplikacja desktopowa na macOS, w której **układasz graf agentów kodujących i go uruchamiasz**.
Definiujesz agenta raz (vendor, model, uprawnienia), przeciągasz go na płótno, łączysz w workflow,
naciskasz `Run` — i widzisz kurowany strumień tego, co się dzieje, zamiast czterech okien terminala.
Zastępuje Superset, Warpa i ręcznie sklejone harnessy.

Poprzednie podejście (poprzedni prototyp, 75 tys. linii Rusta w dwa dni) umarło na złożoność i **nigdy nie
uruchomiło agentów naprawdę równolegle**. Ta historia jest rozpisana w
[`docs/research/projects/`](docs/research/projects/) i jest wiążąca jako lista rzeczy, których tu nie
powtarzamy.

## Stos

| Warstwa | Co |
|---|---|
| Silnik i procesy | Rust 1.96 (edycja 2024), tokio, `process-wrap` na grupy procesów |
| Powłoka desktopowa | Tauri 2 (WKWebView), bez pluginów `shell` i `fs` — procesy odpala Rust |
| Stan na dysku | pliki jako prawda, SQLite (`rusqlite`) wyłącznie jako indeks |
| Interfejs | React 19 + Vite 8 + Tailwind 4 na własnych tokenach, `@xyflow/react` na graf |
| Testy | `cargo test` i Vitest w bramce; Playwright jest zainstalowany, ale jeszcze nie wpięty |
| Vendorzy agentów | Claude Code **i** Codex, oba pierwszej kategorii (decyzja D3) |

## Uruchomienie

```bash
npm install && cargo fetch     # raz, po sklonowaniu

npm run app                    # aplikacja (Tauri dev)
npm run dev                    # sam frontend w przeglądarce, bez Rusta
```

## Co znaczy „zielone"

[`scripts/ci.sh`](scripts/ci.sh) jest **jedynym** źródłem prawdy o tym, co znaczy „zielone".
Uruchamia się lokalnie i wydaje ten sam werdykt, co CI:

```bash
bash scripts/ci.sh             # full == rust ∪ web
bash scripts/ci.sh rust        # fmt, clippy --all-targets, testy, cargo deny, build
bash scripts/ci.sh web         # prettier, tsc, vitest, vite build, słownictwo
```

Kody wyjścia są te same w całym harnessie: `0` przeszło · `1` sprawdzenie padło ·
`2` **my** jesteśmy źle skonfigurowani (brak narzędzia, brak zależności) · `3` przerwane.
Nigdy nie mieszamy `1` z `2`.

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) tylko to opakowuje — nie wymienia ani jednego
sprawdzenia, bo dwie listy się rozjeżdżają. Ma jedno agregujące sprawdzenie wymagane do merge'a.
[`deny.toml`](deny.toml) trzyma politykę łańcucha dostaw: licencje permisywne, copyleft świadomie
poza listą, wildcardy zabronione.

## Pętla pracy

```bash
./worktree.sh <ID>             # własna kopia repo i własna gałąź; wypisuje ścieżkę
./verify.sh before             # DOWIEDŹ, że kryteria są czerwone, zanim cokolwiek napiszesz
./verify.sh quick              # ~20 s, w pętli
./verify.sh full               # przed oddaniem
./review.sh codex              # druga opinia, inny vendor, tylko do odczytu
./repair.sh                    # dokładnie jedna runda poprawek
./integrate.sh <gałąź>         # jedna gałąź naraz, pełna bramka po każdej
```

Albo cały graf naraz: `./ship-task.sh <ID> --agent claude --reviewer codex`.

Trzy rzeczy w tej pętli są nienegocjowalne i opisuje je [`AGENTS.md`](AGENTS.md) §2: `before` musi być
czerwone **z właściwego powodu**, recenzent nie może niczego zatwierdzić ani zablokować, a rund
poprawek jest dokładnie jedna. Bramka to [`harness/gate.py`](harness/gate.py); pojedyncze sprawdzenia
leżą w [`checks/`](checks/), a nazwa pliku steruje tym, w którym poziomie się pojawiają.

## Mapa dokumentów

| Plik | Co w nim jest |
|---|---|
| [`AGENTS.md`](AGENTS.md) | **karta pracy** — 26 ponumerowanych reguł wiążących; czytasz to pierwsze |
| [`docs/DECISIONS-LOCKED.md`](docs/DECISIONS-LOCKED.md) | decyzje człowieka; wygrywają z kartą |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | kształt systemu, maszyna stanów, sufit gęstości |
| [`docs/design/DESIGN.md`](docs/design/DESIGN.md) | tokeny i komponenty; `src/styles/theme.css` jest jego lustrem |
| [`docs/research/projects/00-SYNTHESIS.md`](docs/research/projects/00-SYNTHESIS.md) | co dziedziczymy po trzech poprzednich repo, i czego nie |
| [`docs/research/topics/`](docs/research/topics/) | osiem raportów tematycznych (T1 sterowniki agentów … T8 powłoka desktopowa) |
| `tasks/<ID>.md` | zadania; bramka parsuje z nich wyłącznie `## AC-n` i `check:` (katalog jest jeszcze pusty) |

## Stan repo

Szkielet i harness stoją; **kodu produkcyjnego nie ma jeszcze ani w Ruście, ani w Reakcie** — poza
tokenami motywu w [`src/styles/theme.css`](src/styles/theme.css). Każde sprawdzenie zachowuje się na
tym pustym drzewie uczciwie: mówi jednym zdaniem, że nie ma czego sprawdzać, i przechodzi. Warunek
pominięcia jest zawsze czysto plikowy („nie istnieje ani jeden plik tego typu"), więc pierwszy plik
źródłowy włącza sprawdzenie z powrotem — bez niczyjej decyzji i bez edycji skryptu.
