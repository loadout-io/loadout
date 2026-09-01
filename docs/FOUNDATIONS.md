# Foundations

The rules Loadout is built to. Code, tests, and checks across this repository cite this file by
section number, so the numbering here is load-bearing: `[FOUNDATIONS §2.2]` in a comment means the
table below, and renumbering a section silently breaks that reference.

Three documents sit above the code. [`docs/DECISIONS-LOCKED.md`](DECISIONS-LOCKED.md) records
decisions the owner has made and implementation may not quietly reopen. [`AGENTS.md`](../AGENTS.md)
is the working charter, with numbered rules. This file holds the product's own language and the
engineering conventions that language rests on.

---

## 2. The idea, restated simply

### 2.1 In plain language

You tell Loadout what you want done. Loadout turns that into a short list of **steps** and shows it
to you. You press Start. Each step gets its **own copy of your code** so agents cannot trip over each
other, and Loadout runs the agents you assigned — several at once when the steps do not touch the
same files. Everything each agent does is written down as it happens, in order, and never edited
afterwards. When a step finishes, Loadout **runs the checks itself** — it does not ask the agent
whether it worked. At the end you see three separate things that are never confused with each other:

1. **what the agent said** it did,
2. **what the checks actually found**,
3. **what you approved**.

That separation is the whole idea. An agent can say "done"; only the checks can say "the tests
passed"; only you can say "ship it". No writer may mint another authority's words — an agent's
report is never rendered as a result, and a check's result is never signed with the agent's name.

Two supporting ideas earn their place:

- **Nothing is ever overwritten.** The activity list only grows. That gives crash recovery,
  restore-after-restart, and the per-agent view for free — the per-agent view is just the same list
  filtered to one agent.
- **Notes have to earn trust.** Something an agent claims starts as a *suggestion*. It becomes a
  *confirmed note* only when a second, independent source backs it, and only confirmed notes are
  ever fed back into a future agent's prompt. Without this, one hallucination becomes permanent
  project lore.

### 2.2 The jargon → plain-word table

**This table is binding for Loadout's UI, its `.md` docs, and its error messages.** The left column
may exist in Rust type names. It may not reach a screen. `checks/vocabulary.sh` enforces this, and it
reads assertion messages too, not only rendered strings.

| Their word | Loadout word | Note |
|---|---|---|
| control plane | Loadout | never name the architecture at the user |
| objective | **goal** | one sentence of what you want |
| run | **run** | already plain; keep |
| work item | **step** | a tile in the workflow, a row in the plan |
| attempt | **try** | "try 2 of 3" |
| plan / plan DAG | **the plan** | a numbered list of steps, shown as a block in the transcript |
| loop / `LoopDefinition` | **workflow** | the thing you build in the editor and re-use |
| loop node / `PlanNode` | **step** | same word as work item, deliberately |
| loop edge / dependency edge | **runs after** | an arrow means "after", nothing else |
| ledger / event stream / `DomainEvent` | **activity** | the scrolling list; one entry = one thing that happened |
| projection / reducer | *(never shown)* | internal; the UI just shows "now" |
| authority fact | **who said it** | shown as one word per row |
| `AuthorityKind::RuntimeAdapter` | **agent** | |
| `AuthorityKind::Verifier` / `PolicyKernel` | **Loadout** | the app itself; the checks are Loadout speaking |
| `AuthorityKind::Human` | **you** | |
| claim vs. record | **agent said** vs. **happened** | the single most useful distinction in the UI |
| `agent_finished` | **agent says done** | never render this as "finished" |
| gate / verification gate | **check** | "3 checks passed, 1 failed" |
| `gate_passed` / `gate_failed` | **checks passed** / **checks failed** | |
| `NoTestsExecuted` | **nothing ran** | a distinct, visible outcome — not a pass |
| `InfrastructureError` | **could not run** | distinct from failed |
| verifier | **the checks** | there is no noun "verifier" in the UI |
| evidence receipt | **results** | one per finished step |
| review / advisory verdict | **second opinion** | it can raise concerns; it can never approve or block |
| `review_unavailable` | **no second opinion configured** | one line of UI text, not a domain event |
| artifact (blob) | **file** | |
| snapshot | **saved copy** | "a saved copy of the files at that moment" |
| content-addressed store | *(never shown)* | internal |
| sha256 / digest / binding | **fingerprint** | truncate to 8 chars; full value one click away |
| git worktree | **workspace** | "each step gets its own workspace" |
| `max_parallel` | **how many at once** | and it must be *true* — a limit that does not parallelise is a lie |
| resource lane / `ResourceDemand` | **slot** | "3 Claude slots", "1 code-writing slot" |
| lease | *(never shown)* | internal; surface only as "in use by step 3" |
| outbox / intent / idempotency key | *(never shown)* | internal, entirely |
| `FailureClass` | **why it failed** | |
| `RetryDisposition::BoundedAutomatic` | **retry a few times** | |
| `RetryDisposition::WaitForHuman` | **ask me** | |
| `RetryDisposition::Never` | **stop** | |
| same-fingerprint circuit breaker | **same error again — stopping** | |
| plan approval / canonical approval value | **Start** | plus, if the workflow changed: **"the plan changed since you approved it"** |
| `plan_binding_stale` | **the plan changed** | |
| memory record | **note** | |
| `MemoryStatus::Candidate` | **suggested** | |
| `MemoryStatus::Corroborated` | **confirmed** | |
| `MemoryStatus::Trusted` | **in use** | only "in use" notes go into a prompt |
| `MemoryStatus::Superseded` | **replaced** | |
| retrieval / context manifest | **what this agent was told** | a panel in the agent view |
| MCP server | **tool server** | or just "tools" |
| capability grant / `CapabilityProfile` | **permissions** | |
| `doctor()` / handshake | **check setup** | a button, and the reason it failed |
| adapter / provider | **agent app** | "Claude Code", "Codex" — say the product name |
| agent rail | **the agents list** | right-hand strip; one tile per agent that actually appeared |
| a rail tile's "claim" | **latest note from this agent** | in the agent's own words, marked as such |
| session inspector | **open this agent** | |
| `EventFidelity::DegradedProcess` | **raw output only** | one badge, on the rows it applies to |
| acceptance criterion (`AC-n`) | **check** | same word as gate; there is only one word |
| `red` tier | **before** | "prove the checks fail before we start" |
| `fast` tier | **quick** | |
| `full` tier | **full** | |
| receipt file (`runs/last.json`) | **results file** | |
| integrate | **land** | |
| repair round | **fix round** | |
| probe | **measurement** | |

Two rules that go with the table:

- **One fact, one place.** Pick one live region per fact. The cap is one; a value shown in six
  places has six chances to disagree with itself.
- **Never fake-complete free prose.** Autocomplete only over closed sets the client actually holds —
  workflow names, flags, repositories it has seen. A fake completion is worse than none, because it
  looks like knowledge the client does not have.

---

## 3. Rust way of working

Each item is a convention with the reason it exists. Most were bought with an incident.

**Crate layout.** Virtual workspace root; `src-tauri` is the app; extra binaries are separate
members with `default-members = ["src-tauri"]` so the inner loop never compiles them. `src/main.rs`
is a handful of lines calling `lib::run()`, so `cargo test --lib` is the whole test surface. Profile
decisions carry their incident: `[profile.dev.package."*"] debug = false` cuts roughly a third of
incremental time and most of the link-time RAM peak that can otherwise freeze the machine. Split
modules early — a `commands/mod.rs` that reaches five figures of lines is the named mistake.

**Tauri boundary.** Every command is a two-line `#[tauri::command]` shim over
`pub(crate) fn <name>_inner(state: &AppState, …)`, because `State<'_, AppState>` cannot be built in a
unit test and `&AppState` can. One `generate_handler!` list, one `app.manage`; a command that
compiles but is not registered is silently un-callable, and that is the most common IPC bug. Tauri
matches `invoke` arguments **by name**, so a renamed parameter is a silent no-op rather than a
compile error — `checks/invoke-args.sh` exists for exactly this. Enumerate capabilities explicitly;
never `core:default`.

**Events.** One module owns the names, the typed `#[serde(rename_all = "camelCase")]` payload
structs, and the `emit_*` helpers. Emit failures warn and continue. Throttle progress with a cursor.
**Never emit during `setup`** — Tauri does not buffer and the webview has not called `listen()` yet;
write state and let the frontend pull.

**Error handling.** One `thiserror` enum, one `pub type Result<T>`, and a manual `Serialize` to the
bare `Display` string. `anyhow` only behind `Other(#[from])`. On top of the prose, a machine-readable
`[code]` allowlist so the frontend maps code to sentence, and **anything uncoded renders a generic
sentence — deny by default**. Tests pin the wire string so a rename breaks the build. Startup returns
`Err` and shows a dialog; it never panics.

**Async.** tokio, through `tauri::async_runtime::spawn`/`spawn_blocking` where an `AppHandle` is in
play. **A std `Mutex` is never held across an `await`** — say so on the field itself.
`spawn_blocking` is **not** a concurrency limiter: heavy work acquires an owned `Semaphore` permit
and **moves it into the blocking closure**. Cancellation is a monotonic generation counter, never a
global bool, because a bool leaks across operations — and cancel is a **value**, never an `Err`.

**SQLite.** `rusqlite`, hand-written SQL, `struct Db { conn: Mutex<Connection> }`. Open order is
load-bearing: extensions, open, pragmas, **`busy_timeout` on every connection**, then `migrate()`.
The bundled build does not carry the textbook defaults — set `foreign_keys` and `busy_timeout`
yourself. Migrations have no framework and no version table: `CREATE TABLE IF NOT EXISTS` plus
`add_column_if_missing`, idempotent, with a test that proves idempotence, and every test database
built by running the **real** `migrate()` against an in-memory connection. `DROP`,
`ALTER … DROP COLUMN`, and row rewrites are forbidden. Verify before destroy on any at-rest
transform: prove the new bytes read back identical *before* deleting the old ones. The activity
table carries `UNIQUE(run_id, sequence)` and reject-update/reject-delete triggers, so "append-only"
stays true even for a connection that bypasses the Rust API. Files are the source of truth; SQLite
is a rebuildable index.

**Testing.** Unit tests inside the lib crate; large suites split into files and re-attached with
`#[cfg(test)] #[path = "tests/x.rs"] mod x;` so they keep `use super::*`. **Assert the serialized
wire shape**, not a Rust round-trip: compare the key *set* of `serde_json::to_value(&dto)`, and
remember that `skip_serializing_if` changes that set. A data-carrying enum needs **both**
`rename_all` and `rename_all_fields`; missing the second ships snake_case fields to a camelCase
frontend, and the first six fixes will go to the wrong layer. A hand-written frontend mock *defines*
a shape; it does not verify one. Every regression that reached a user gets a named deterministic
oracle, and any oracle that could go vacuous ships with a control test proving the failure still
fails.

**Subprocess.** One `build_*_command` seam per vendor, so a test can assert through `get_args()`
that every real spawn carries the flags. `env_clear()` plus a minimal PATH plus an explicit
passthrough allowlist. Prompts and secrets ride **stdin only** — never argv, never a temp file,
never a log, because argv is readable by any local process. `process_group(0)`, kill the *group*,
**prove it dead** (`kill(-pgid, 0)` returning ESRCH) and fail closed while unproven.
`kill_on_drop(true)`. Bounded pipe readers started **before** stdin is written, and off the stack —
two 8 KB buffers in an `async fn` are enough to trip `large_futures`. One wall-clock deadline
covering write, EOF, and wait, plus a **separate, smaller** reap budget. A failure cooldown so a
broken child cannot cause a respawn storm.

**Lint gates.** [`scripts/ci.sh`](../scripts/ci.sh) is the single source of truth for "green"; the
GitHub workflow only wraps it, with `full == rust ∪ web` by construction and one aggregating required
check. `deny.toml` from day one: allowlisted licenses, `wildcards = "deny"`,
`unknown-git`/`unknown-registry = "deny"`, and every advisory ignore carrying a justification and a
removal condition. `cargo clippy --all-targets` is **banned from the inner loop** — it thrashes the
build profile — and runs once in the gate, which is why a lint that only `--all-targets` sees can
stay hidden through a green quick pass. Lint policy lives in a `[workspace.lints]` table, not only in
a CI shell string, so the editor sees it too.

**Comment the why, especially the incident.** Nearly every non-obvious line here carries a dated
rationale. It is the cheapest convention on the list and the reason the tree stays navigable.

---

## 5. The task-file template

Location `tasks/<ID>.md`; copied to `TASK.md` in the run's workspace and committed as the branch's
first commit. The gate parses **only** `## AC-n` and `check:`; everything else is for the human and
the agent.

````markdown
# <ID> — <one-line title; the file's own stem must appear here>

> **Gate:** <omit unless gated. "Started by a human." / "Blocked on <decision>." /
> "Run by <prompt file>.">

<Two to five sentences: why this task exists and what is actually hard about it. Name the silent
failure mode — the wrong answer nobody notices. This paragraph is what stops a model producing the
fluent-and-average version.>

**Read first:** `docs/research/topics/T<n>-<topic>.md` (<what it contains and why it decides
something here>), `docs/design/DESIGN.md` (<the specific tokens or behaviours>),
`docs/PLAN.md` invariants <n>, <n>.

## Who runs this

- **Agent:** <the Loadout agent, e.g. `rust-core` / `react-ui` / `harness`>
- **Second opinion:** <the other vendor — never the one that wrote it>
- **Run artifacts:** `runs/<ID>/` (transcript, results file, plan) — never `$TMPDIR`

## What this task owns

- `<path/to/dir>` — <what lives there, and the one constraint that holds across it>
- `<path/to/one.rs>`, `<path/to/two.test.tsx>`

Every path in these bullets must also appear in the OWNS block below, and no other task may claim
any of them.

## Invariants

- **<n> — <the invariant, restated in this task's terms>.** <How it gets broken quietly here.>

## Patterns

`docs/patterns/<nn>-<name>.md`, `docs/patterns/<nn>-<name>.md`.

## Budget

<Only if this task has one. Copy the number and its id from SCOPE.md — never restate it in new
words.>

## Acceptance criteria

## AC-1 <the behaviour, as an observable fact, not as a task>
check: <one shell command running exactly ONE test file, by path — never a -t/--test-name filter>

<Two to six lines: the specific cases with their values. Cite the research file for any number you
did not derive here.>

*The weak assertion:* <the exact implementation that passes this check and fails this criterion, and
the extra assertion that discriminates.>

## AC-2 …
check: …

<…>

*The weak assertion:* <…>

<Five to eight criteria, numbered from 1 with no gaps. Twelve means two are halves of one. If a
criterion's only evidence is a lint or a grep over a clean tree, it is not a criterion — it cannot
go red before the work exists.>

## Deliberately out of scope

<Everything a reader would reasonably expect and will not get, each with where it went (another task
id) or why it is cut. "Partial" must be a stated edge, not an unfinished one.>

<!-- OWNS
path/to/dir
path/to/one.rs
-->
````

---

## 6. The review checklist

Review every change and every design document against this. Each line names a failure that has
actually shipped somewhere, in this repository or in one it learned from.

**Concurrency and correctness**

- [ ] Does "how many at once" actually run things at once, or is it dispatch width over a single
      worker? Parallel agents are the premise of this product; a limit that does not parallelise is
      a lie in the UI.
- [ ] Is the in-flight limit *keyed* — one effect per try id, one per run id, capped at "how many at
      once"? Deriving a try number from a count read in an earlier lock hold mints the same id and
      the same workspace path twice, and races two workspace creations.
- [ ] Is there a single-worker, claim-one-per-pass, idle-poll loop anywhere? Fine for a durable
      control plane; wrong for a desktop app that must feel instant.
- [ ] Are two parallel steps allowed to write overlapping paths? Refuse it at save time, before the
      first process starts.
- [ ] Is more than one heavy `cargo` or `rustc` running on this machine? Several concurrent links
      pin the memory compressor and freeze it with swap still at zero.

**Over-modelling**

- [ ] Is there a trait with exactly one implementation?
- [ ] Is there a schema-migration path in code that has never had a second version?
- [ ] Are there three state machines where one would do?
- [ ] Does one concept have three content hashes and a string format? One hash of the plan JSON,
      compared before dispatch, is all of the value at a fraction of the cost.
- [ ] Does editing a skill invalidate an approved plan? That is hostile while someone is iterating.
      Show a "config changed" banner instead.
- [ ] Is a self-referential check being counted as a test — one that re-derives your own hash and
      calls it a pass, so the check list can be non-empty? Allow **"no checks configured"** as an
      honest state.
- [ ] Is a disabled feature being built?

**Honesty that turns into noise**

- [ ] Is any fact stated in more than one place at once? The cap is **one**.
- [ ] Is a wire enum printed as user-visible text?
- [ ] Is any word on screen missing from the §2.2 table?
- [ ] Is a full SHA-256 or a composite binding string rendered verbatim in the primary flow?
      Truncate, copy on demand, full value one click away.
- [ ] Are transcript rows expanded by default? Collapsed by default is the requirement, and the
      alternative is how a screen reaches triple-digit text elements against a cap of 60.
- [ ] Is an absence explained in two or three sentences of policy prose where a phrase would do?
- [ ] How many regions animate off one event? Pick one live region per fact.
- [ ] How much chrome sits above the first content? The ceiling is in
      [`docs/ARCHITECTURE.md`](ARCHITECTURE.md) §7, and `scripts/density-audit.mjs` enforces it.
- [ ] How many navigation metaphors are on screen? More than one means more than one answer to
      "where am I".

**Dead ends and fabrication**

- [ ] Does any control ship without a handler? (Invariant 16.)
- [ ] Is any UI drawing a relationship the data does not contain? (Invariant 17.) Fake edges between
      hardcoded coordinates are the classic form.
- [ ] Is a UI affordance built on a field that does not exist, or synthesised from an unrelated one?
- [ ] Is a permanently-empty cell being displayed?
- [ ] Does a capability handshake **refuse** where a warning would do? Detecting a vendor's features
      by string-matching its `--help` output turns every upstream rename into a hard refusal.

**Harness self-deception**

- [ ] Does any test assert that a file *contains a string*, rather than that a behaviour holds?
- [ ] Is any check green on an exit code alone, with no passing count in its output? A green exit
      code without proof that tests ran is not a green check.
- [ ] Does any file get written that no script reads?
- [ ] Does the evaluation or self-test live inside the system it measures? A refactor can delete it
      wholesale and nothing will notice.
- [ ] Is policy reimplemented in a per-vendor adapter, rather than one core with thin adapters?
      That is how a scanner silently dies in one vendor and stays green in the other.
