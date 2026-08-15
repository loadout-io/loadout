# 02 — Udowodnij, że proces nie żyje

**Osierocony agent to błąd finansowy, nie higieniczny.** `claude`, który przeżył anulowanie,
dalej pali twój limit — w tle, bez okna, bez linii w logu.

## Trzy rzeczy, które trzeba zrobić razem

### 1. Startuj w grupie procesów

```rust
// process-wrap: ta sama linia wywołania na uniksie i na Windows.
// Bez tego zabijasz `claude`, a jego dziecko `bash -c "cargo test"` żyje dalej.
let mut cmd = TokioCommandWrap::with_new(program, |c| { c.args(args); });
#[cfg(unix)]    cmd.wrap(ProcessGroup::leader());
#[cfg(windows)] cmd.wrap(JobObject);
```

### 2. Zabijaj grupę, nie proces

```rust
kill(Pid::from_raw(-pgid), Signal::SIGTERM)?;   // uwaga na minus
// łaska
kill(Pid::from_raw(-pgid), Signal::SIGKILL)?;
```

### 3. Zażądaj dowodu

```rust
// Sygnał 0 nic nie wysyła — tylko pyta „czy ta grupa istnieje".
// ESRCH = nie istnieje = naprawdę nie żyje.
match kill(Pid::from_raw(-pgid), None) {
    Err(Errno::ESRCH) => Ok(Dead),
    _ => Err(StillAlive),     // FAIL CLOSED: dopóki nie ma dowodu, traktuj jako żywy
}
```

**Nieudowodniona śmierć to nie śmierć.** Nie loguj „zatrzymano" na podstawie tego, że
`kill()` zwrócił `Ok`.

## Pułapka, która wygląda niewinnie

```rust
tokio::time::timeout(dur, child.wait()).await   // ← anuluje ZADANIE RUSTA
```

To anuluje future'a. Proces systemowy żyje dalej. Każda ścieżka limitu czasu musi przejść
przez eskalację zabijania z supervisora — nigdy przez samo `timeout()`.

## Test

Odpal dziecko, które forkuje wnuka (`bash -c 'sleep 300 & sleep 300'`), anuluj,
i sprawdź `ESRCH` na całej grupie. Oznacz `#[ignore]`, żeby nie chodził w wewnętrznej pętli,
i uruchamiaj w bramce.

Motyw z meetnotes: osierocone procesy testowe z PPID 1, najstarszy sprzed 21 godzin.
