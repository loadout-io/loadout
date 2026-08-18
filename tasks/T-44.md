# T-44 — Gdzie umiejetnosc ma wyladowac: w tym projekcie albo wszedzie

**Caly mechanizm zakresu jest napisany, przetestowany i NIEOSIAGALNY z aplikacji.** `Scope::Project`
istnieje od T-18, `place::plan` i `place::remove` przyjmuja go, `Roots` ma na to pole, a jedno
kryterium T-18 dowodzi, ze pod korzeniem repo powstaja te same dwie nazwy katalogow. Produkcja
wola to wylacznie z `Scope::Global`, bo jedyny konstruktor `Roots` w kodzie ma wpisane `None`.

Zmierzone 2026-08-19 na wyladowanym trunku:

```
commands/skills.rs:166-172   fn global_roots(library) -> Roots { project: None, .. }   // prywatny
commands/skills.rs:388       place::plan(&import.skill, Scope::Global, &roots)          // jedyny wolacz
commands/skills.rs:222       place::destinations(Scope::Global, &roots.home, ...)       // lista
skills/place.rs:438-440      if scope == Project && roots.project.is_none() -> NoProjectRoot
tests/it/skills_place_destinations.rs:208    jedyne uzycie Scope::Project w calym drzewie
```

T5 §8.3 ma ten wybor w projekcie formularza od 2026-08-15 („Available in: This project /
Everywhere"), a §11 na liscie MVP. Nie powstal, bo okno nie ma czym przyslac zakresu, a Rust nie
ma skad wziac korzenia projektu.

## Dlaczego to nie jest „dodaj enum do sygnatury"

**Zakres bez korzenia projektu nie odmawia — pisze w losowe miejsce.** `destinations(Scope::Project,
home, None)` nie zawiera warunku i oddaje sciezki **wzgledne**: `.claude/skills` i `.agents/skills`.
Do dysku nie dochodza dzis tylko dlatego, ze `plan` odmawia wczesniej. Kazda implementacja, ktora
zawola `destinations` albo `apply` z pominieciem `plan`, zapisuje umiejetnosc pod katalogiem
roboczym procesu — czyli w `npm run tauri dev` pod `src-tauri/.claude/skills`. To jest to samo
„zgadywanie cwd", ktore doc `Roots.project` nazywa wprost, tylko zrobione przez `Path::join`
na pustce. Kryterium musi to sprawdzic jawnie.

**Droga zapisu bez drogi odczytu jest gorsza niz brak funkcji.** `list_skills_inner` czyta dzis
wylacznie katalogi globalne. Umiejetnosc zapisana „w tym projekcie" nie pojawi sie na liscie,
wiec czlowiek nie zobaczy jej i nie bedzie mial jak jej zabrac — a sekcja pisze do zywej
konfiguracji jego narzedzi agentowych. To jest dokladnie ten ksztalt defektu, ktory to repo
naprawialo trzy razy w tym tygodniu (T-26, T-27, T-38): mechanizm wylądował, nikt go nie zawolal.
Dlatego zakres wchodzi **w obie strony naraz**: zapis, lista, usuniecie.

**Korzen projektu nie jest wymyslany po stronie Rusta — jest tym samym korzeniem, ktorego uzywa
bieg.** `run_workflow` bierze `folder: Option<String>` z okna i sprawdza go `AppState::project_for`
(bezwzglednosc, istnienie, katalog-nie-plik), a okno bierze go z `activeWorkspace()?.folder`
(`src/state/workspaces.ts`) — jednej funkcji, ktora odpowiada na pytanie „gdzie pracujemy".
Instalacja jedzie **tym samym szwem**, nie drugim. Dwie odpowiedzi na „ktory to projekt" rozjada
sie pierwszego dnia, w ktorym ktos przelaczy karte.

Czyli zadanie polega na **doprowadzeniu istniejacego zakresu do okna i domknieciu odczytu**,
nie na napisaniu drugiego rozmieszczania.

**Read first:**
`src-tauri/src/skills/mod.rs:97-120, 187-202` (`Scope`, `Roots`, `NoProjectRoot` slowo w slowo) ·
`src-tauri/src/skills/place.rs:112-124` (`destinations` — dlaczego bez korzenia daje sciezki
wzgledne) · `src-tauri/src/skills/place.rs:410-471, 576-621` (`plan` i `remove` z zakresem) ·
`src-tauri/src/commands/skills.rs:166-172, 214-253, 374-391, 400-440` (wszystkie cztery miejsca
z `Scope::Global`) · `src-tauri/src/ipc.rs:470-493` (`project_for` i jego zdania odmowy) ·
`src/state/workspaces.ts` (`activeWorkspace`) · `src/sections/run/launch.ts:39-55` (wzor zdania,
kiedy nie ma wybranego folderu) · `docs/research/topics/T5-skill-portability.md` §8.3 i §12 pyt. 6 ·
`AGENTS.md` niezmienniki 4, 13, 16, 23.

## Kto to robi

- **Agent:** `rust-core` na `commands/skills.rs`, potem `react-ui` na sekcji — jeden worktree,
  dwa kroki, jedna bramka.
- **Druga opinia:** inny vendor niz pisarz (D3); recenzentowi powiedz wprost, zeby sprawdzil, czy
  przy braku otwartego projektu cokolwiek powstaje pod katalogiem roboczym procesu.
- **Artefakty biegu:** `runs/T-44/`

## Zalezy od

**T-42 i T-43.** Oba dotykaja `src/sections/skills/index.tsx`, `src/state/skills.ts`,
`src/sections/skills/io.ts` i tabeli `WIRES` — trzy galezie na tych samych czterech plikach daja
konflikt w kazdym lądowaniu. Kolejnosc: T-42, T-43, potem to zadanie.

## Co to zadanie posiada

- `src-tauri/src/commands/skills.rs` — zakres i korzen projektu w instalacji, w liscie i w
  usuwaniu; `global_roots` przestaje byc jedynym konstruktorem `Roots`.
- `src-tauri/src/ipc.rs` — **waski mandat**: skorupy `install_skill`, `list_skills` i
  `delete_skill` dostaja argument folderu, sprawdzany istniejacym `AppState::project_for`.
  Zadnej nowej komendy, zadnej zmiany w `generate_handler!`.
- `src/sections/skills/index.tsx` — wybor „gdzie to ma wyladowac", postawiony nad kontrolka
  dodania, i zdanie o miejscu policzone z tego wyboru.
- `src/sections/skills/io.ts`, `src/state/skills.ts` — trzy krawedzie i akcje magazynu niosa
  wybor oraz folder z `activeWorkspace()`.
- `src/sections/commands-wired.test.ts` — **waski mandat**: wolno zmienic `given` i `call`
  wylacznie w trzech istniejacych wierszach (`install`, `listSkills`, `remove`). Ani jednego
  wiersza nie wolno usunac, ani jednego innego dotknac.
- `src-tauri/tests/it/main.rs` — **waski mandat**: ten plik masz w OWNS WYLACZNIE po to, zeby
  dopisac dwa wiersze `mod skills_scope_two_roots;` i `mod skills_scope_round_trip;` w porzadku
  alfabetycznym. Zadnej innej zmiany.
- 3 pliki testow wymienione przy `check:`.

**Czego to zadanie NIE dotyka:** `src-tauri/src/skills/place.rs` i `mod.rs` (T-18 — `Scope`,
`Roots`, `destinations`, `plan`, `remove` i komunikat `NoProjectRoot` sa gotowe i wystarczaja),
`src/state/workspaces.ts` (T-24 — `activeWorkspace` jest publiczna i wystarcza), `.gitignore`
(patrz „Swiadomie poza zakresem"), `src/sections/skills/review-card.tsx` — wybor stoi NAD karta,
w ekranie, wiec propsy karty sie nie zmieniaja i `review-card.test.tsx` zostaje zielony bez ani
jednej zmiany.

## Niezmienniki

- **13 — jeden fakt, jedno miejsce.** „Gdzie pracujemy" ma jedna odpowiedz: folder aktywnego
  workspace'u, sprawdzony `project_for`. *Jak sie lamie po cichu:* instalacja czyta `LOADOUT_PROJECT`
  albo `current_dir()` na wlasna reke i zapisuje umiejetnosc w innym miejscu niz to, w ktorym
  pracuje bieg.
- **4 — pliki sa prawda.** Lista pokazuje to, co lezy w katalogach, ktore agent naprawde czyta —
  wiec przy wybranym projekcie musi czytac oba korzenie, a nie jeden.
- **16 — kontrolka bez skutku nie wchodzi do repo.** Wybor „ten projekt", ktory nic nie zmienia
  w tym, gdzie plik ladeuje, jest gorszy niz jego brak.
- **23 — polityka w jednym rdzeniu.** Nazwy `.claude/skills` i `.agents/skills` liczy dalej
  wylacznie `place::destinations`. *Jak sie lamie po cichu:* `roots.project.join(".claude/skills")`
  wpisane w warstwe komend, bo „tak krocej".

## Kryteria akceptacji

**Jak zaczerwienic to poprawnie.** Sygnatury najpierw z trywialnie zla wartoscia (`Scope::Global`
zwracany zawsze), nigdy `todo!()` — `clippy::todo` jest `deny`. Pliki testow zaczynaj od
`#![allow(clippy::unwrap_used, clippy::expect_used)]` z powodem. Sciezki docelowe w asercjach
**wypisz literalnie** (`home.join(".claude/skills")`), nie bierz ich z `DESTINATION_DIRS` — tak robi
`skills_ingest_no_exec.rs:104-114` i ma do tego zapisany powod: kryterium sprawdzajace implementacje
jej wlasna tablica przechodzi po kazdej zmianie tej tablicy, lacznie z literowka. Po stronie okna:
`renderToStaticMarkup`, zasiew `setState`, atrapa granicy IPC.

## AC-1 Dwa zakresy pisza w dwa korzenie, a bez projektu nie powstaje NIC — nigdzie
check: cargo test --test it skills_scope_two_roots::
expect: (\d+) passed

Fikstura: `tempfile::TempDir` rozgaleziony na `home`, `project` i `data`, kopia kanoniczna
umiejetnosci w `data/skills/<name>/`, sentinel: druga umiejetnosc juz zainstalowana w korzeniu
globalnym. Trzeci katalog tymczasowy ustawiony jako katalog roboczy procesu testu.

Asercje: (a) zakres „wszedzie" tworzy `home/.claude/skills/<name>/SKILL.md` i
`home/.agents/skills/<name>/SKILL.md`, i **nie** tworzy niczego pod korzeniem projektu;
(b) zakres „ten projekt" tworzy te same dwie nazwy pod korzeniem projektu i **nie** dotyka
korzenia globalnego — sentinel ma po tym te same bajty i ten sam czas modyfikacji; (c) zakres
„ten projekt" bez znanego korzenia jest odmowa zdaniem z rdzenia i **po probie nie istnieje ani
jeden nowy plik**: ani pod domem, ani pod projektem, ani pod katalogiem roboczym procesu — to
ostatnie jest tu wlasciwa asercja, bo `destinations(Scope::Project, home, None)` oddaje sciezki
wzgledne i implementacja, ktora pominie `plan`, zapisze umiejetnosc wlasnie tam; (d) folder podany
z okna, ktory nie jest istniejacym katalogiem bezwzglednym, jest odmowa **tym samym** zdaniem, co
przy uruchomieniu biegu — jedna odpowiedz na „ktory to projekt", nie dwie.

*Slaba wersja:* asercja, ze `plan` zwrocil `Err` przy braku korzenia. Przechodzi na dzisiejszym
kodzie, bez ani jednej linii zmiany, bo `plan` juz tak robi — i nie mowi nic o warstwie, ktora ma
`plan` zawolac. Rozstrzyga: liczenie plikow w trzech katalogach po probie, wlacznie z katalogiem
roboczym procesu.

## AC-2 Co zapisane w projekcie, to widoczne i zabieralne — a globalne zostaje na miejscu
check: cargo test --test it skills_scope_round_trip::
expect: (\d+) passed

Fikstura: ta sama umiejetnosc o tej samej nazwie zainstalowana DWA razy: raz globalnie, raz
w projekcie. Obok, w korzeniu projektu, katalog o trzeciej nazwie napisany przez kogos innego
(bez wpisu w sidecarze).

Asercje: (a) lista przy wybranym projekcie niesie umiejetnosci z OBU korzeni, kazda raz — nie dwa
razy, bo instalacja pisze w dwa katalogi vendorow i zbior nazw jest jeden; (b) ta sama lista bez
wybranego projektu niesie wylacznie globalne: lista odpowiada na pytanie „co widzi agent pracujacy
tutaj", a nie „co kiedykolwiek zapisalismy"; (c) usuniecie z zakresu projektu zabiera obie kopie
projektowe i **zostawia globalne nietkniete** — ta sama nazwa w dwoch zakresach to dwie rzeczy;
(d) katalog napisany przez kogos innego nie jest kasowany, a zdanie odmowy nazywa sciezke —
polityka „czy to nasze" zostaje w `place::remove` i sidecarze, a nie powstaje tu drugi raz.

*Slaba asercja:* sprawdzenie, ze po usunieciu katalog projektowy nie istnieje. Przechodzi
implementacja, ktora skasowala oba zakresy naraz, i taka, ktora skasowala cudzy katalog o tej
samej nazwie. Rozroznia: sentinel globalny z tymi samymi bajtami i cudzy katalog obok.

## AC-3 Wybor stoi tam, gdzie decyzja, i jedzie razem z zapisem
check: npx --no-install vitest run src/sections/skills/where-it-goes.test.tsx
expect: (\d+) passed

Fikstura: `renderToStaticMarkup` na `<SkillsScreen>` z przejrzana umiejetnoscia w magazynie, dwa
warianty magazynu workspace'ow: z wybranym folderem i bez zadnego. Granica IPC podmieniona atrapa.

Asercje: (a) wybor stoi w tej samej sekcji co karta przegladu i **przed** kontrolka dodania:
ostrzezenie i decyzja o miejscu widoczne po dodaniu nie sa ostrzezeniem (dzis zdanie
`WHERE_IT_LANDS` stoi wlasnie tam i to jest wzor); (b) zdanie o tym, gdzie to wyladuje, zmienia sie
z wyborem i jest **jedno** — dwa zdania o jednym fakcie to niezmiennik 13; (c) dodanie wysyla
wybor oraz folder wziety z `activeWorkspace()`, nie wpisany w test, i obie wartosci sa
w argumentach wywolania nazwanego z `src-tauri/commands.golden.txt`; (d) bez wybranego workspace'u
opcja „ten projekt" nie jest do wybrania, a zdanie mowi, co zrobic, nazywajac kontrolke DOKLADNIE
tak, jak stoi na ekranie (`Add a workspace` z bocznego menu) — zdanie odsylajace do kontrolki
nazwanej inaczej niz na ekranie jest instrukcja, ktorej nie da sie wykonac.

*Slaba wersja:* asercja, ze w markupie sa dwa `<input type="radio">`. Przechodzi na wyborze, ktory
nigdzie nie jedzie — czyli na kontrolce bez skutku, w miejscu, gdzie skutkiem jest zapis do zywej
konfiguracji narzedzi czlowieka. Rozstrzyga: odczyt argumentow z atrapy granicy.

## Swiadomie poza zakresem

- **`.gitignore` w projekcie czlowieka.** T5 §12 pyt. 6 zostawia otwarte, czy `.agents/skills/`
  ma jechac do repo zespolu, czy do `.gitignore`. To zadanie **zapisuje pliki i nie dotyka
  `.gitignore`** — decyzja nalezy do czlowieka i jest zapisana nizej.
- **Zakres per krok workflow.** Krok ma wlasny wybor umiejetnosci (T-13, `skills-row.tsx`) i to
  jest inne pytanie.
- **Przenoszenie miedzy zakresami.** „Zainstaluj tez wszedzie" to osobna droga; tutaj zakres jest
  wyborem przy dodawaniu.

**Decyzja czlowieka, ktora to zadanie zaklada (orchestrator 2026-08-19).** `docs/ARCHITECTURE.md`
§6a regula 3 mowi dzis: „Agenci, workflow i umiejetnosci sa globalne (`~/.loadout/`), wiec karta nic
by tam nie znaczyla". Po tym zadaniu zostaje to prawda o BIBLIOTECE (kopia kanoniczna dalej lezy
w `~/.loadout/skills/`) i przestaje byc prawda o MIEJSCU DOCELOWYM. Regula dostaje jedno zdanie
poprawki w osobnym commicie, nie z wnetrza biegu — `docs/` nie jest w bloku OWNS tego zadania.

<!-- OWNS
src-tauri/src/commands/skills.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/skills_scope_two_roots.rs
src-tauri/tests/it/skills_scope_round_trip.rs
src-tauri/tests/it/main.rs
src/sections/skills/index.tsx
src/sections/skills/io.ts
src/state/skills.ts
src/sections/commands-wired.test.ts
src/sections/skills/where-it-goes.test.tsx
-->
