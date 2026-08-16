# T-36 — Przelotka nie podnosi diala bezpieczenstwa

`docs/DECISIONS-LOCKED.md` D6 stawia dwa ograniczenia na przelotke `vendorOptions`, zeby nie stala
sie dziura. Drugie brzmi doslownie:

> **Przelotka nie omija diala bezpieczenstwa.** Pole „co agent moze zrobic z plikami" jest
> tlumaczone przez nas na flagi vendora; przelotka nie moze go podniesc.

Zmierzone na wyladowanym trunku (przeglad zewnetrzny 2026-08-16): filtr eskalacji stoi **wylacznie**
na przelotce kroku workflow. Definicja agenta ma wlasna przelotke, ktora `library::agents::vendor_args`
tlumaczy prosto do argv **bez ani jednego sprawdzenia** — wiec plik `~/.loadout/agents/*.json`
z `"--dangerously-skip-permissions": ""` omija dial calkowicie.

Utajone, bo `vendor_args` nie ma dzis produkcyjnego wolajacego — co samo w sobie jest druga wada
tej samej rodziny (niezmiennik 16: kontrolka bez handlera). Ale filtr ma byc **zanim** ktokolwiek
ja podepnie, a nie potem: podpiecie bedzie jednolinijkowe i nikt przy nim nie przeczyta D6.

**Read first:**
`docs/DECISIONS-LOCKED.md` D6 (oba ograniczenia przelotki) · `AGENTS.md` niezmienniki 23 (polityka
w jednym rdzeniu — filtr ma byc JEDEN, nie dwa) i 16 · miejsce, w ktorym filtr juz istnieje dla
kroku workflow (`workflow/`), bo to jego lista jest zrodlem prawdy.

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** inny vendor niz pisarz (D3).
- **Artefakty biegu:** `runs/T-36/`

## Co to zadanie posiada

- `src-tauri/src/library/agents.rs` — **waski mandat**: filtr w `vendor_args` i nic wiecej.
  Piętnastu pol agenta, `resolve`, `capture` ani zapisu plikow nie dotykamy.
- Dwa pliki testow wymienione przy `check:`.

Filtr ma byc **wspolny z tym, ktory juz dziala na kroku workflow** — jedna lista, jedno miejsce.
Druga kopia listy zakazanych flag to sposob, w jaki skanowanie sekretow po cichu umarlo
w repo zrodlowym (niezmiennik 23).

## Kryteria akceptacji

## AC-1 Flaga eskalujaca z definicji agenta nie dochodzi do argv, i wiadomo ktora
check: cargo test --test agents_vendor_args_filtered

Agent z `vendorOptions` zawierajacymi flagi eskalacji dla obu vendorow — `bypassPermissions`,
`--dangerously-skip-permissions`, `danger-full-access` — plus jedna flaga **niewinna**. Asercje:
w zwroconym argv nie ma ani jednej flagi eskalujacej; niewinna **jest**; a funkcja **nazywa** te,
ktore odrzucila (cicha odmowa uczy uzytkownika, ze przelotka nie dziala, zamiast ze zostala
zablokowana).

*Slaba asercja:* sprawdzenie, ze argv jest krotsze. Przechodzi na implementacji, ktora wycina
przypadkowa flage. Dyskryminuje: obecnosc niewinnej **i** brak kazdej eskalujacej z osobna,
po nazwie.

## AC-2 Obie przelotki uzywaja TEJ SAMEJ listy, nie dwoch kopii
check: cargo test --test agents_vendor_args_one_policy

Dla kazdej flagi z listy zakazanych: przelotka **kroku workflow** ja odrzuca **i** przelotka
**definicji agenta** ja odrzuca. Jedna petla po jednej liscie, dwa wywolania na kazdy element.

To kryterium istnieje, bo dokladnie tak ta dziura powstala: filtr napisano raz, w jednym miejscu,
i drugie miejsce o nim nie wiedzialo. Test po jednej liscie pęka w dniu, w ktorym ktos doda flage
tylko do jednej kopii.

*Slaba asercja:* dwa osobne testy, kazdy ze swoja lista wpisana recznie. Przechodza obok siebie
w nieskonczonosc, rozjezdzajac sie po cichu — czyli odtwarzaja te sama wade pietro wyzej.
Dyskryminuje: **jedna** lista jako zrodlo obu polowek asercji.

## Swiadomie poza zakresem

- **Podpiecie `vendor_args` do prawdziwego biegu** — nie ma dzis wolajacego i to jest osobna luka
  (niezmiennik 16). Tutaj powstaje filtr, ktory ma czekac gotowy.
- **Pierwsze ograniczenie D6** (kolizja z flagami, ktore Loadout ustawia sam) — ma juz kryterium
  w T-11.

<!-- OWNS
src-tauri/src/library/agents.rs
src-tauri/tests/agents_vendor_args_filtered.rs
src-tauri/tests/agents_vendor_args_one_policy.rs
-->
