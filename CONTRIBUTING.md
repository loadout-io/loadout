# Contributing to Loadout

Thanks for your interest. Loadout is licensed under **GNU AGPL-3.0** — see [`LICENSE`](LICENSE).

## Before you write code

Read [`AGENTS.md`](AGENTS.md). It is the working charter of this repository, and its rules are
numbered because plans and reviews cite them by number. Above it sits only
[`docs/DECISIONS-LOCKED.md`](docs/DECISIONS-LOCKED.md), which records decisions the owner has
already made and which implementation may not quietly reopen.

Two conventions surprise people, so they are worth stating up front:

- **One fact lives in one place.** If a value, a rule, or a sentence already exists somewhere,
  reference it rather than copying it. Two sources of truth is a defect this repo avoids on
  purpose, because you always read the stale copy.
- **A check asserts what a person can see.** A test that passes while the screen is wrong is a
  gap in the oracle, not a green check. If you find one, say so — do not soften the assertion.

## Running it

Requirements: macOS, Node.js with npm, and the Rust toolchain pinned by
[`rust-toolchain.toml`](rust-toolchain.toml).

```bash
npm install
cargo fetch
npm run app
```

## The gate

[`scripts/ci.sh`](scripts/ci.sh) is the single source of truth for the repository gate. GitHub
Actions calls the same script rather than maintaining a second list of checks, so a green run on
your machine means the same thing as a green run in CI.

```bash
bash scripts/ci.sh          # complete Rust + web gate
bash scripts/ci.sh rust     # formatting, clippy, tests, dependency policy, build
bash scripts/ci.sh web      # formatting, types, browser integration tests, vocabulary, build
```

A pull request is expected to be green before review. If a check is wrong, argue with it in the
pull request — changing a check so that your code passes is the one change that will always be
refused.

## Opening a pull request

1. Discuss anything non-trivial in an issue first.
2. Branch from `main` and keep the change focused on one thing.
3. Make `bash scripts/ci.sh` green.
4. Add a `Signed-off-by: Name <email>` line to your commits (the
   [Developer Certificate of Origin](https://developercertificate.org/)), which states that you
   wrote the contribution and may submit it under the project's license.

## Licensing of contributions

Contributions are accepted under AGPL-3.0, the same license the project ships under. There is no
Contributor License Agreement at this time; if that changes, this file will say so before any
contribution is affected.
