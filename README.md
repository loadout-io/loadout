<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="112" height="112" alt="Loadout icon">
</p>

<h1 align="center">Loadout</h1>

<p align="center">
  <strong>Build and run visual workflows for coding agents.</strong><br>
  Compose Claude Code, Codex, checks, commands, checkpoints, and loops on one canvas — then watch the work happen in one place.
</p>

<p align="center">
  <a href="https://github.com/JakubGawr/Loadout/releases/latest"><strong>Download for macOS</strong></a>
  ·
  <a href="https://github.com/JakubGawr/Loadout/actions/workflows/ci.yml">CI</a>
  ·
  <a href="docs/ARCHITECTURE.md">Architecture</a>
</p>

![A ten-step multi-agent research workflow on the Loadout canvas](https://github.com/JakubGawr/Loadout/releases/download/v0.1.0/loadout-release-canvas.png)

## One graph owns the work

Loadout is a native macOS control room for coding agents. Define an agent once, place it on a
workflow canvas, connect the steps, and run the graph against a project. Execution order,
parallel branches, fan-in, retries, checkpoints, and handoffs all come from the saved graph — no
stage is hard-coded into the engine.

Claude Code and Codex are first-class peers. A workflow can mix vendors and models, give every
agent a different working style and tool set, and keep implementation and review independent.

## What ships in 0.1

- **Visual workflows** — agent steps, local commands, checks, checkpoints, loops, conditional
  paths, retries, and reusable saved graphs.
- **Real parallel execution** — independent branches overlap in time instead of taking turns
  behind a single worker.
- **A live Run screen** — agent activity, durable history, questions, spawned commands, output,
  diagnostics, and spend stay together.
- **Reusable agents** — choose the vendor, model, reasoning depth, timeout, file access, tools,
  connections, and skills for each role.
- **Project-scoped skills and memory** — curate instructions and useful notes on disk, then see
  which context was actually used by a run.
- **Triggers and recovery** — schedule workflows and recover interrupted work without inventing a
  second execution engine.
- **Evidence you can inspect** — run receipts, full attachments, handoffs, death proofs, and a
  diagnostic bundle remain available after the terminal output is gone.

![A local command started from the Run screen remains visible and inspectable](https://github.com/JakubGawr/Loadout/releases/download/v0.1.0/loadout-release-running-demo.png)

## Built to fail honestly

Loadout treats orchestration failures as product failures, not terminal noise:

- overlapping write scopes are refused before the first process starts;
- cancellation terminates the whole process group and verifies that it is dead;
- timeouts travel through the same supervised shutdown path;
- prompts and secrets go through stdin, never command-line arguments;
- child environments are rebuilt from an explicit allowlist;
- unknown vendor events are recorded and ignored instead of crashing the run;
- a green exit code without proof that tests ran is not accepted as a green check;
- files are the source of truth, while SQLite remains a rebuildable index.

## Install

Loadout 0.1 is currently packaged for Apple Silicon Macs.

1. Download [`Loadout_0.1.0_aarch64.dmg`](https://github.com/JakubGawr/Loadout/releases/download/v0.1.0/Loadout_0.1.0_aarch64.dmg).
2. Open the DMG and drag Loadout to Applications.
3. Launch it normally — the app and DMG are signed with Apple Developer ID, notarized, and
   stapled.
4. Install and authenticate at least one supported agent CLI: Claude Code or Codex.

DMG SHA-256:

```text
29a29e02e0adb30971e1b29225b2c435f4ee9bd38506a3b1b69037d18f9b8e10
```

See the [v0.1.0 release](https://github.com/JakubGawr/Loadout/releases/tag/v0.1.0) for the full
release notes and verification receipt.

## Run from source

Requirements: macOS, Node.js with npm, and the Rust toolchain pinned by
[`rust-toolchain.toml`](rust-toolchain.toml).

```bash
npm install
cargo fetch
npm run app
```

Use `npm run dev` when you only need the browser frontend without the Tauri backend.

## Verification

[`scripts/ci.sh`](scripts/ci.sh) is the single source of truth for the repository gate. GitHub
Actions calls the same script instead of maintaining a second list of checks.

```bash
bash scripts/ci.sh          # complete Rust + web gate
bash scripts/ci.sh rust     # formatting, clippy, tests, dependency policy, build
bash scripts/ci.sh web      # formatting, types, browser integration tests, vocabulary, build
```

The v0.1.0 artifact is bound to commit
[`88b004124bb36dbc5d0d37e5d353ba1c2dbf10f6`](https://github.com/JakubGawr/Loadout/commit/88b004124bb36dbc5d0d37e5d353ba1c2dbf10f6),
whose [full CI gate passed](https://github.com/JakubGawr/Loadout/actions/runs/33073026820).

## Repository guide

| Path | Purpose |
|---|---|
| [`src/`](src/) | React interface: Run, Workflows, Agents, Skills, Memory, and Triggers |
| [`src-tauri/src/engine/`](src-tauri/src/engine/) | graph execution, scheduling, drivers, supervision, recovery, and evidence |
| [`src-tauri/src/store/`](src-tauri/src/store/) | file-backed state and the rebuildable SQLite index |
| [`e2e/`](e2e/) | browser-level interaction and visible-behavior checks |
| [`checks/`](checks/) | deterministic repository checks discovered by the gate |
| [`harness/`](harness/) | task-contract runner, review, bounded repair, and receipts |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | current system shape and invariants |
| [`docs/DECISIONS-LOCKED.md`](docs/DECISIONS-LOCKED.md) | owner decisions that constrain the implementation |
| [`docs/STATUS.md`](docs/STATUS.md) | chronological build and task record |

## Current status

Phase 7 is complete and v0.1.0 is released. The repository contains the production Rust engine,
Tauri desktop shell, React interface, both agent drivers, recovery and supervision paths, browser
interaction coverage, and the full task harness. This README describes the shipped tree rather
than the original project skeleton.
