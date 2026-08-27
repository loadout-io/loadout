# T-147 — Startup reaper zachowuje pełny odcisk dowodu

Świeży standalone następca zamkniętego T-143. T-143 miało uczciwe czerwone `before`, oba
produkcyjne commity i zielony prawdziwy target T-135, lecz Harness zatrzymał implementację
przed bramką: każdy z dwóch nowych speców spadł z 7 do 6 linii asercji. Gałąź jest dowodem,
nie źródłem do lądowania w całości.

Po własnym uczciwym `before` wolno selektywnie zastosować wyłącznie produkcyjne commity
`277d0c9` oraz `64915e0`, jeśli pasują do świeżego szkieletu. Nie przejmuj `TASK.md`, commita
speców/szkieletu `d1f9d8a`, testowego `e81eb06` ani całej gałęzi. Nowe targety muszą zachować
co najmniej po siedem konkretnych asercji od certyfikacji aż do końca biegu.

Zadanie nie zmienia polityki produktu. Wydziela w `supervisor.rs` jeden, neutralny rdzeń
`reap_group_with_signaler`, który przyjmuje `Term`, `Probe`, `Kill` oraz odpowiedzi
`Delivered`, `NoSuchGroup`, `Refused`. `reap_group` pozostaje cienkim adapterem prawdziwego
`killpg`: wyłącznie ESRCH mapuje na `NoSuchGroup`, każdy inny błąd — także EPERM — na
`Refused`. Ten sam rdzeń wywołują produkcja i oba standalone targety.

**Read first:** `src-tauri/src/engine/supervisor.rs` (`reap_group`, `signal_group`,
`wait_for_group_to_disappear`) · `src-tauri/tests/t135_startup_cleanup_escalates.rs` ·
`tasks/T-143.md` · `runs/T-143/{assertions-certified.tsv,assertions-now.tsv}`.

## Kto to robi

- **Agent:** Codex przez jeden pełny bieg Harnessu, po zamknięciu T-143.
- **Druga opinia:** osobny Codex na innym modelu, tylko do odczytu; właścicielski wyjątek D3.

## AC-1 Niejednoznaczna odpowiedź nigdy nie prowadzi do KILL
check: cargo test --test t147_startup_reaper_refuses_ambiguous_probe
expect: (\d+) passed

Target wykonuje deterministycznie bez prawdziwych procesów dwa scenariusze. `TERM → Refused`
daje `Alive` i ślad dokładnie `[Term]`. `TERM → Delivered`, potem `Probe → Refused` daje
`Alive` i ślad dokładnie `[Term, Probe]`. W żadnym przypadku nie pojawia się `Kill`, kolejna
sonda ani zgadnięte `Dead`. Skrypt odpowiedzi odrzuca dodatkowe wywołanie i po scenariuszu
potwierdza, że nie została niewykorzystana odpowiedź. Test nie importuje `libc` ani stałych
platformowych i zachowuje minimum **7** linii asercji.

## AC-2 Dead po KILL pochodzi wyłącznie z produkcyjnej sondy ESRCH
check: cargo test --test t147_startup_reaper_proves_after_kill
expect: (\d+) passed

Przy zerowych limitach rdzeń nadal wykonuje co najmniej jedną sondę przed oceną czasu.
Sekwencja `Term=Delivered, Probe=Delivered, Kill=Delivered, Probe=NoSuchGroup` zwraca `Dead`
i ma ślad dokładnie `[Term, Probe, Kill, Probe]`. Kontrola z ostatnim `Probe=Delivered` zwraca
`Alive`; samo `Kill=Delivered` nigdy nie wystarcza. Skrypt odrzuca dodatkowe akcje i wymaga
zużycia wszystkich odpowiedzi. Nie istnieje prawdziwy proces ani zewnętrzny `kill -0`, więc
późniejsza śmierć fixture nie może zazielenić testu. Target zachowuje minimum **7** linii
asercji.

## Uczciwe `before`

Kontrakt tworzy neutralne publiczne enumy, jawne limity i doc-hidden sygnaturę szwu z
`todo!()` przed `verify.sh before`. Oba kompletne targety zawierają od początku co najmniej
po siedem finalnych asercji, kompilują się i padają w runtime na szkielecie. Brak symbolu,
brak targetu albo prawdziwy proces nie są prawidłową czerwienią.

## Wyłączenia

Nie zmieniać targetu T-135, recovery ani historii. Nie tworzyć osobnej testowej polityki.
Szew jest publiczny wyłącznie dla standalone integration targetu, ma `#[doc(hidden)]` i
datowany komentarz. Platforma oraz `libc` pozostają wyłącznie w `supervisor.rs`. Nie
przenosić targetów T-143 ani nie zmniejszać liczby asercji po certyfikacji.

<!-- OWNS
tasks/T-147.md
src-tauri/src/engine/supervisor.rs
src-tauri/tests/t147_startup_reaper_refuses_ambiguous_probe.rs
src-tauri/tests/t147_startup_reaper_proves_after_kill.rs
-->
