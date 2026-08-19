# T-52 — Izolacja kroku ma być drzewem gita, nie ręcznym kopiowaniem

`fresh-copy` kopiuje dziś projekt plik po pliku własnym walkerem (`copy_project_into`,
`commands/run.rs`). Zmierzone 2026-08-19 na `~/Projects/meetnotes`:

| co | wartość | dlaczego to boli |
|---|---|---|
| bieg `20260819-165926` | **odmowa, bez `run.json`** | kopia padła, zanim powstał pierwszy zrzut |
| na czym padła | `.claude/worktrees/murmur-server` → `~/Projects/murmur-server` | dowiązanie do katalogu |
| ile zdążyła | 13 MB, kończy się na `.claude/worktrees` | nic nie ruszyło |
| droga powrotna pracy | **nie istnieje** | `copy_project_into` to jedyny transport w tym pliku |
| co kopia pomija | `.git`, `node_modules`, `target` | w meetnotes to 22 z 23 GB — agent dostaje drzewo, którego nie da się zbudować |
| klon APFS tego samego drzewa | 274 MB w **3,7 s, zero zajętego miejsca** | bloki współdzielone; przechodzi dowiązanie do katalogu, zerwane dowiązanie i FIFO |

Przyczyna odmowy jest jednozdaniowa: `entry.file_type()` **nie podąża** za dowiązaniem, więc
dowiązanie do katalogu wygląda na „nie katalog" i leci do `fs::copy`, a `fs::copy` za nim podąża
i odmawia. Ale naprawa tego jednego kształtu nic nie rozstrzyga: `pnpm`, `python -m venv`, `git
worktree` i worktree Claude Code robią takie wpisy same, a po nich przyjdą xattr, hardlinki
i uprawnienia. Piszemy `cp` ręcznie i przegrywamy z systemem plików po kawałku.

**Rozstrzygnięcie właściciela 2026-08-19:** „własna kopia" przestaje znaczyć kopię bajtów i zaczyna
znaczyć **własne drzewo robocze**. Projekt będący repozytorium dostaje `git worktree` na własnej
gałęzi — izolację utrzymuje git, a praca ma dokąd wrócić. Folder, który repem nie jest, dostaje
klon systemowy, który przeżywa każdy kształt pliku.

## Czego to zadanie NIE zmienia

- **Odmowa zostaje głośna.** T-33 AC-2 (`fresh_copy_degrades_loudly`) ma pozostać zielone bez
  jednej zmiany w swoim pliku: kiedy izolacji naprawdę nie da się zrobić, nie startuje ani jeden
  proces, a komunikat nazywa powód i krok.
- **Trzy tryby folderu zostają** (`Folder::Project`, `FreshCopy`, `Pick`) i nazwa w interfejsie
  też. Zmienia się to, CZYM jest własne drzewo, nie ile jest opcji.
- **Niezmiennik 12 zostaje.** `workflow::check` dalej odmawia zapisu dwóch równoległych kroków
  celujących w ten sam folder.
- **Ciężkich katalogów ignorowanych przez gita nie wnosimy.** Drzewo robocze niesie pliki śledzone;
  `node_modules` i `target` w nim nie ma, tak samo jak nie ma ich dzisiaj. Krok, który ich
  potrzebuje, ma tryb „katalog projektu".

## Rozstrzygnięcia, których kryteria pilnują

1. **Gałąź, nie odczepiona głowa.** Drzewo powstaje na gałęzi nazwanej biegiem i krokiem. Odczepiona
   głowa zostawia pracę dokładnie tam, gdzie zostawia ją dzisiejsza kopia: nigdzie.
2. **Twoja niescommitowana praca jedzie z Tobą.** Drzewo startuje z HEAD, a różnica plików
   **śledzonych** jest do niego nakładana. Agent widzi to, co Ty w edytorze — inaczej pisze
   przeciwko drzewu sprzed Twoich zmian i konflikt jest pewny.
3. **Pliki nieśledzone są NAZWANE, nie połknięte.** Nie wchodzą do drzewa (git ich nie zna), więc
   bieg mówi, ile ich było, zanim ruszy. Cicha strata jest tu gorsza od braku funkcji.
4. **Krok, który niczego nie zmienił, nie zostawia śmiecia.** Gałąź i drzewo bez ani jednej zmiany
   znikają po biegu; z choćby jedną zmianą — zostają i są wymienione w podsumowaniu biegu.

## AC-1 Krok pracuje we własnym drzewie gita, a projekt tego nie widzi
check: cargo test --test it worktree_isolates_the_step::

Katalog projektu **będący repozytorium** z dwoma plikami w commicie. Puść dwa kroki w trybie własnej
kopii; pierwszy zmienia jeden plik i tworzy drugi. Asercje: (a) oba kroki widziały pliki z commita
na starcie; (b) zmiana pierwszego **nie jest widoczna** dla drugiego; (c) katalog **oryginalny** jest
nietknięty — `git status` w nim jest taki sam przed i po; (d) katalog roboczy kroku jest prawdziwym
drzewem gita, czyli `git -C <kopia> rev-parse --is-inside-work-tree` odpowiada `true`; (e) kontrola
przeciw pustemu czytaniu: mniej niż dwa pliki widziane przez krok to błąd testu, nie zieleń.

*Słaba asercja:* sprawdzenie, że katalogi robocze są różne. Dwa puste katalogi też są różne.
Dyskryminuje **obecność plików projektu w obu** i to, że drzewo jest drzewem gita, a nie kopią.

## AC-2 Niescommitowana praca jedzie do drzewa, a nieśledzona jest nazwana
check: cargo test --test it worktree_carries_your_uncommitted_work::

Repozytorium z jednym plikiem w commicie. **Przed biegiem**: zmień ten plik bez commita i dołóż
drugi, nieśledzony. Asercje: (a) krok widzi plik śledzony w wersji **zmienionej**, nie tej
z commita; (b) pliku nieśledzonego w drzewie **nie ma**; (c) bieg powiedział, ile plików
nieśledzonych zostawił — liczba jest w stanie biegu, nie tylko w dzienniku; (d) katalog oryginalny
dalej ma obie zmiany, czyli nakładanie różnicy niczego z niego nie zabrało.

*Słaba asercja:* sprawdzenie, że plik istnieje. Plik z commita też istnieje i różni się jedną
linią. Dyskryminuje **treść** pliku w drzewie.

## AC-3 Folder, który repem nie jest, przeżywa każdy kształt pliku
check: cargo test --test it isolation_survives_every_file_shape::

Katalog **bez** `.git`, a w nim: zwykły plik, podkatalog, **dowiązanie do katalogu**, **zerwane
dowiązanie** i **kolejka FIFO**. Asercje: (a) bieg rusza — żaden z tych kształtów nie jest odmową;
(b) zwykły plik jest w kopii z tą samą treścią; (c) dowiązania są w kopii **dowiązaniami**, nie
kopiami swojego celu — inaczej katalog po drugiej stronie wchodzi do każdej kopii każdego kroku;
(d) kontrola przeciw pustemu czytaniu: test sam sprawdza, że w źródle NAPRAWDĘ stoją wszystkie
cztery egzotyczne kształty — mniej niż cztery to błąd testu, nie zieleń, bo `mkfifo` albo
dowiązanie mogło się nie udać i wtedy kryterium mierzy pusty katalog.

*Słaba asercja:* test na samo dowiązanie do katalogu. To jest jeden zmierzony kształt z pięciu,
a klasa błędu jest szersza niż jego pierwszy przedstawiciel.

## AC-4 Praca kroku jest osiągalna po biegu, a pusty krok nie zostawia śmiecia
check: cargo test --test it worktree_leaves_the_work_reachable::

Dwa kroki w repozytorium: pierwszy zapisuje plik, drugi nie zmienia nic. Asercje: (a) po biegu
istnieje gałąź niosąca zmianę pierwszego kroku i jej nazwa wymienia bieg i krok; (b) zmiana jest
na niej **osiągalna z gita** — `git log <gałąź>` widzi commit, a `git diff` wobec bazy pokazuje
plik; (c) po drugim kroku nie zostaje ani gałąź, ani wpis w `git worktree list`; (d) kontrola:
`git worktree list` w projekcie nie rośnie o kroki, które nic nie zrobiły.

*Słaba asercja:* sprawdzenie, że katalog roboczy dalej istnieje. Katalog istnieje też dzisiaj —
i jest dokładnie tym miejscem, z którego nikt nigdy nie wyjął pracy. Dyskryminuje **osiągalność
z gita**.

## AC-5 Kiedy izolacji zrobić się nie da, nie rusza ani jeden proces
check: cargo test --test it isolation_names_what_it_could_not_do::

Wymuś warunek, w którym drzewa zrobić się nie da (repozytorium bez ani jednego commita, więc nie ma
z czego założyć drzewa). Asercje: (a) żaden krok nie wystartował; (b) komunikat nazywa **krok**
i **powód**, a nie kod błędu systemu; (c) powód mówi, co z tym zrobić; (d) kontrola: `RunError::Io`
w tej ścieżce jest błędem testu — przezroczysty wariant oddaje zdanie o systemie plików, nie
o człowieku (ten sam powód, dla którego powstał `RunError::NoFreshCopy`, T-33 AC-2).

*Słaba asercja:* `assert!(result.is_err())`. Przechodzi na implementacji, która wywala bieg bez
powiedzenia dlaczego.

<!-- OWNS
src-tauri/src/commands/run.rs
src-tauri/src/commands/mod.rs
src-tauri/src/commands/isolate.rs
docs/ARCHITECTURE.md
src-tauri/tests/it/main.rs
src-tauri/tests/it/worktree_isolates_the_step.rs
src-tauri/tests/it/worktree_carries_your_uncommitted_work.rs
src-tauri/tests/it/isolation_survives_every_file_shape.rs
src-tauri/tests/it/worktree_leaves_the_work_reachable.rs
src-tauri/tests/it/isolation_names_what_it_could_not_do.rs-->
