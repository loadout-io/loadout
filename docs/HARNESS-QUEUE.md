# Kolejka poprawek harnessu

Zmiany gotowe do naniesienia, których **nie wolno nanieść w trakcie biegu**.

**Od `4f9a558` ta kolejka jest w dużej mierze zbędna.** Q-1 sprawiło, że `ship-task.sh`
i `scripts/build-loop.sh` odpalają się z przypiętej kopii (`exec bash "$snap"`), więc edycja
w trakcie biegu przestała psuć biegnący proces. Kolejka zostaje dla tego, co nadal jest
niebezpieczne w trakcie biegu, i dla zapisu, czego świadomie **nie** mechanizujemy.

Co jeszcze jest wrażliwe na moment:

| Co | Dlaczego nie w trakcie |
|---|---|
| `checks/*.sh` | zmieniają bramkę pod zadaniem, które jest przez nią właśnie sądzone |
| `harness/gate.py` | to samo, plus `integrate.sh` sądzi trunk przed merge'em. A jeśli **musisz** (orchestrator naprawiający bramkę w trakcie fali): zapisuj przez `tmp` + `os.replace`. `verify.sh` robi `exec python3 harness/gate.py` przy **każdym** wywołaniu, więc zapis częściowy daje bramce śmieci zamiast oracle'a. Zwykły zapis Pythona gubi też bit wykonywalności — `chmod 755` po |
| cokolwiek, gdy biegną strażnicy | `harness/guards.sh` przerywa się kodem 2 na brudnym drzewie, meldując „restore failed" — czyli twoja edycja wygląda jak wada strażnika |
| `TASK.md` gałęzi | jest bajt w bajt porównywany z wersją z commita planu (kod 1 i kod 2) |

Bezpieczne w każdym momencie: `docs/**`, `AGENTS.md`, `.claude/**` (proces agenta wczytał
ustawienia przy starcie), nowe pliki, których nikt jeszcze nie woła.

---

## Q-6 — zegar scienny nie odroznia „wolne" od „wisi"

**Stan: OTWARTE. Zdiagnozowane 2026-08-17, naprawione tylko kalibracja.**

`run_one` mierzy sprawdzenie zegarem sciennym i po dwoch przekroczeniach budzetu melduje
`rc=124` z tekstem „it is waiting for something that is not going to arrive". To jest
DIAGNOZA, na ktora bramka nie ma dowodu: identycznie wyglada sprawdzenie zawieszone i
sprawdzenie, ktoremu ktos zabral procesor.

Zmierzone tego dnia: `cargo test --tests` to 119 binariow, 444 s na maszynie bezczynnej
i **1121 s** na maszynie, na ktorej obok chodzi fala. Przy budzecie 600 s bramka zabila
suite dwa razy, zaswiecila trunk na czerwono i zablokowala ladowanie T-31 -- nie majac ani
jednego padajacego testu. Poszla na to poltorej godziny diagnozy, w tym podejrzenie o
zawieszenie rzucone na `claude_rate_limit`, czyli test, ktory jest czystym `include_str!`
i parserem i nie ma czym wisiec.

Naniesione teraz: budzet `full-test` z 600 na 1800 s, **z pomiaru**. To usuwa objaw i jest
uczciwe (liczba zamiast przeczucia), ale nie usuwa przyczyny: kazdy budzet zgadnie zle,
bo obciazenie maszyny nie jest wlasnoscia kodu.

**Wlasciwa naprawa: liczyc CISZE, a nie czas.** `cargo test` drukuje „Running tests/x.rs"
przy kazdym binarium, wiec postep jest obserwowalny. Sprawdzenie, ktore nadal pisze, zyje --
zabijac wolno dopiero po N sekundach BEZ ANI JEDNEGO bajtu, plus twardy sufit (np. 3x budzet)
na wypadek petli, ktora gada. Wtedy „wolne" i „wisi" przestaja byc tym samym sygnalem.

**Dlaczego nie teraz:** to zmiana w `_run`, czyli w rdzeniu uruchamiania KAZDEGO sprawdzenia,
a tego dnia bramka dostala juz cztery poprawki (lane szeregowy, zamek poziomu full,
`--keep-going`, budzet). Blad w `_run` bylby gorszy niz problem, ktory naprawia. Do zrobienia
na spokojnej bramce, z kontrola dwustronna: sprawdzenie gadajace i wolne ma przezyc,
sprawdzenie ciche ma zginac.

## Q-7 — 122 cele testowe to 122 linkowania tej samej biblioteki

**Stan: ZAMKNIETE 2026-08-28 — regula kontraktu, nie kalibracja budzetu.**

Kalibracja (budzet 9000 s) usuwala objaw i wracala, bo PRZYCZYNA byla w kontrakcie kryterium:
AGENTS.md §2a regula 2 wymagala globalnie unikalnej SCIEZKI PLIKU na kazde kryterium, a kazdy
plik wprost w `src-tauri/tests/` to osobne binarium linkujace cala biblioteke. Scalenie do
jednego celu `it` zrobiono 2026-08-17 (122 -> 1) i **odroslo do 60**, bo regula zamawiala nowy
plik przy kazdym kolejnym zadaniu; w `tasks/*.md` bylo w sumie **462** takie linie.

Naprawa: §2a regula 1 mowi teraz, ze kryterium rustowe wskazuje MODUL jedynego celu
(`check: cargo test --test it <modul>::`), a nowego pliku wprost w `src-tauri/tests/` nie
zakladamy. Bramka umiala to czytac od 2026-08-17 (`CARGO_TARGET` w gate.py mapuje
`--test it <modul>::` na `src-tauri/tests/it/<modul>.rs`) — brakowalo wylacznie reguly,
ktora KAZE tak pisac. Etap planu w `ship.sh` dostaje ta instrukcje wprost, z pomiarem.

Co zostaje do zrobienia raz, na spokojna glowe: przeniesc 60 istniejacych celow z
`src-tauri/tests/*.rs` do `src-tauri/tests/it/` jako moduly. Kazdy przeniesiony cel to
~60 s mniej w `full-test` przy kazdym ladowaniu.

*Zapis ponizej zostaje jako historia pomiaru.*

`src-tauri/tests/` ma 122 pliki, a kazdy plik w `tests/` to OSOBNE binarium, ktore linkuje cala
biblioteke razem z zaleznosciami Tauri. Zmierzone dwoma sposobami: pelny przebieg to ~2 h, a
kontrolowany pomiar jednego celu po dotknieciu `commands/run.rs` daje 60 s i 62 s.

**Same testy trwaja 6,0 s.** Cala reszta to linkowanie.

Konsekwencja jest strukturalna: kazde zadanie dotykajace `commands/` uniewaznia wszystkie 122
cele, wiec `full-test` kosztuje dwie godziny. Przy budzecie 1800 s nie mial jak przejsc nigdy —
tak padly T-29, T-32 i ladowanie po T-33.

Naniesione teraz: budzet 9000 s, z pomiaru. To usuwa objaw i jest uczciwe, ale nie usuwa
przyczyny: ladowanie kazdej galezi trwa teraz dwie godziny.

**Sprawdzone i odrzucone:** `[profile.test] debug = "line-tables-only"`. Na macOS informacja
debugowania zwykle dominuje czas linkera — tutaj nie: 60/62 s wobec 62/71 s, roznica w granicach
szumu. Zmiana bez zmierzonego zysku nie zostaje w repo.

**Wlasciwa naprawa: mniej celow.** Scalenie 122 plikow w kilkanascie binariow (po podsystemie:
`store`, `workflow`, `run`, `agents`, `skills`, `memory`, `engine`, `ipc`) tnie czas linkowania
o rzad wielkosci. Kryteria zadan wskazuja jednak konkretne cele przez `cargo test --test <cel>`,
wiec scalenie wymaga przepisania `check:` w kilkudziesieciu plikach zadan i sprawdzenia, ze
`quick-scope` oraz regula „jedna sciezka spec, jedno kryterium" dalej trzymaja. To refaktor na
spokojna glowe, nie zmiana miedzy zadaniami.

## Puste

Q-1 … Q-4 naniesione w `4f9a558`. Poprawka muteksu cargo w `689e432`.

---

## Q-5 — ROZSTRZYGNIĘTE: nikt nie montował sekcji

Decyzja Jakuba 2026-08-15: **nowe zadanie T-25**, wariant A (konwencja zamiast rejestru).

`src/App.tsx` szuka `src/sections/<id>/index.tsx`. Każde zadanie sekcji tworzy własny `index.tsx`
w poddrzewie, które już posiada — zero plików dzielonych. `src/ui/sections.tsx` zostaje bez zmian
i **nie** dostaje pola `component`: to by zrobiło z niego drugi wspólny kręgosłup obok `lib.rs`,
z tą samą klasą kolizji, a front — inaczej niż Rust — niczego takiego nie wymaga.

T-25 stoi w kolejce **przed T-08**, bo T-08 jest pierwszym zadaniem sekcji. Dowód end-to-end nie
został w T-25 (nie ma tam czego montować, a atrapa zostałaby w repo na zawsze — niezmiennik 17):
poszedł do T-08 jako AC-8, wraz z `src/sections/run/index.tsx` w jego OWNS. Przekazanie ma
mechanizm, nie tylko zdanie — to była cała wada, którą Q-5 opisywało.

---

## Czego świadomie NIE mechanizujemy

**„Jedna komenda na wywołanie Bash".** Kusi, żeby zrobić z tego hak `PreToolUse`, ale hak
odmawiający też kosztuje turę — dokładnie tę samą, którą kosztuje odmowa uprawnień. Zysku
zero, a dochodzi ryzyko fałszywej odmowy na poprawnym łańcuchu. Zostaje promptem, bo to
zachowanie, a nie stan, który da się wykryć i naprawić.

**Asercja, że hak formatujący jest podpięty.** `.claude/**` jest dla biegu zabronione do
zapisu — jedynym, kto może go odpiąć, jest orchestrator. Sprawdzenie pilnowałoby wyłącznie
mnie, a `checks/MANIFEST` kazałby dopisać wpis. Ceremonia większa niż ryzyko.

**Prompt rundy naprawczej kontraktu — zostaje promptem, ale nie sam.** Instrukcja „napraw
szkielet tak, żeby kryterium padało od razu" jest zachowaniem, nie stanem: nie da się jej
wykryć hakiem ani sprawdzeniem, bo dopóki model nie napisze poprawki, nie ma czego oglądać.
Więc prompt — ale **jedyna droga na skróty, jaką ta instrukcja otwiera, dostała mechanizm**:
„spraw, żeby padało INACZEJ" da się przeczytać jako „asertuj mniej", i to
`assertion_fingerprint` łapie po stronie stanu, a nie prośby. To jest wzorzec, o który
chodzi w niezmienniku 28: prompt opisuje intencję, mechanizm pilnuje jedynego wyjścia awaryjnego.

**„Kryterium węższe niż niezmiennik, którego broni" — NIE UMIEM tego zmechanizować.**
Wzorzec opisany w `docs/STATUS.md`, cztery przypadki 2026-08-16. Przeszedłem kolejność
z niezmiennika 28 i wszystkie trzy odpadły, więc zapisuję **dlaczego**, a nie tylko „zostaje
promptem":

- **hak** naprawiający stan po cichu — nie ma czego naprawić, plik jest składniowo poprawny
  i test przechodzi; wada jest w tym, o co pyta, a nie w tym, jak wygląda;
- **sprawdzenie w `checks/`** — musiałoby porównać *uzasadnienie kryterium* z *tym, co asercja
  faktycznie wykonuje*. To jest sąd o sensie, nie o stanie. Sprawdzenie przybliżone (np. „każdy
  `data-*` wspomniany w prozie ma mieć test klikający") dałoby fałszywe alarmy na kryteriach,
  które są w porządku, a i tak nie złapałoby przypadku AC-3, gdzie asercja była **dosłownie
  zgodna** z prozą i obie były nieprawdą o świecie;
- **uprawnienia** — nie ma czego zabronić.

Zostaje więc **recenzja innego vendora**, i to nie jest porażka mechanizacji, tylko dokładnie
ten mechanizm, który research wskazał: wszystkie realne defekty na **zielonej** bramce w repo
źródłowym znalazł recenzent innego vendora. Dziś to potwierdzone czwarty raz — uwagę
o `Store::open` na tej samej ścieżce zgłosił recenzent, a nie żadne sprawdzenie.

Jedna rzecz, którą **da się** zrobić i jest tania: przy pisaniu kryterium pytać wprost
„czy ta asercja sprawdza niezmiennik, czy jego najłatwiejszy objaw?". To wchodzi do prozy
zadania, nie do harnessu.

**Union merge poza `src-tauri/src/lib.rs` — świadomie NIE.** `engine/mod.rs` (68 wierszy)
i `memory/mod.rs` (212) mimo nazwy niosą prawdziwy kod, więc `merge=union` mógłby skleić tam
dwie wersje funkcji zamiast dwóch deklaracji. `src/App.tsx` tak samo, a dodatkowo po T-25
przestał być punktem dopisywania (sekcje montują się przez konwencję
`src/sections/<id>/index.tsx`). Kiedy któryś zacznie **realnie** konfliktować — zmierz i wtedy
dopisz. Nie wcześniej: to ta sama pokusa, co 583 MB na agenta przepisane z raportu zamiast
zmierzone na tej maszynie.

**Automatyczne rozwiązywanie konfliktów w `integrate.sh` — NIE.** Reguła union zdejmuje
z lądowania konflikt na kręgosłupie, i to wystarczy. Każdy **inny** konflikt przy lądowaniu
znaczy, że dwa zadania naprawdę sięgnęły po te same wiersze — a to jest pytanie do człowieka,
nie sytuacja do zautomatyzowania. `integrate.sh` ma się wtedy zatrzymać i wypisać pliki.

**Sufit czekania na muteks NIGDY powyżej budżetu sprawdzenia — zmierzone 2026-08-16, boleśnie.**

Podnosiłem `LOADOUT_CARGO_LOCK_WAIT=2400` przy każdym biegu fali, żeby kolejkowanie nie dawało
fałszywej czerwieni. Przy lądowaniu `T-27` to się odwróciło: `full-test` ma budżet **600 s**
(`CHECK_TIMEOUT_OVERRIDE`), a sufit czekania **2400 s** — więc check czekał na zamek, został
zabity na własnym budżecie, ponowiony, znowu czekał, znowu zabity, i zameldował „it is waiting
for something that is not going to arrive". Ono właśnie miało przyjść. Trunk zaświecił się na
czerwono **przed** merge'em i nic nie wylądowało.

Przy domyślnym sufcie 300 s ten sam stan daje **exit 2** („zajęty muteks — nie umiem sprawdzić"),
czyli odpowiedź prawdziwą, którą `integrate.sh` umie przeczytać. Reguła:

> **`LOADOUT_CARGO_LOCK_WAIT` musi być wyraźnie mniejszy niż najmniejszy budżet sprawdzenia
> cargo (dziś 420 s dla `quick-clippy`).** Powyżej tej granicy kolejkowanie przestaje być
> kolejkowaniem i staje się zwisem — a zwis czyta się jak wina kodu.

Praktycznie: podnoś go do 240 s przy szerokiej fali, nigdy ponad. Albo — prościej i pewniej —
**landuj na cichej maszynie**: `integrate.sh` i tak wymaga wyłączności na trunku.

**Sufit czekania na muteks cargo (300 s) — decyzja ODWRÓCONA 2026-08-16.**

Pierwotnie: „jeśli żywe cargo trzyma zamek pięć minut, to nie jest sytuacja do przeczekania,
tylko sygnał, że fala jest za szeroka; podniesienie limitu ukryłoby ten sygnał".

To rozumowanie było poprawne **dla biegu szeregowego** i przestaje obowiązywać przy wachlarzu.
Przy sześciu zadaniach naraz kolejkowanie na muteksie jest **projektowanym zachowaniem**
(niezmiennik 26 przepuszcza jeden ciężki cargo), a nie objawem czegokolwiek. Sufit 300 s
zamieniłby wtedy normalną kolejkę w `exit 2` i fałszywą czerwień u ostatniego w kolejce.

Nie zmieniamy domyślnej wartości — zmieniamy ją **tam, gdzie zmienia się założenie**:
wachlarz eksportuje `LOADOUT_CARGO_LOCK_WAIT=2400`, bieg szeregowy zostaje przy 300 s.
Jedno założenie, jedno miejsce, obie wartości uzasadnione.
