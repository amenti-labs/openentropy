# AGENTS.md

Practical instructions for humans and coding agents working in this repo.

## Scope

- Applies to the entire repository.
- Primary goal: keep behavior and docs aligned across Rust core, CLI, Python bindings, HTTP server, and docs site.

## Source Of Truth Map

- Core API and entropy behavior: `crates/openentropy-core/**`
- CLI commands and flags: `crates/openentropy-cli/src/main.rs` and `crates/openentropy-cli/src/commands/**`
- HTTP endpoints: `crates/openentropy-server/src/lib.rs`
- Python bindings surface: `crates/openentropy-python/src/lib.rs` + `*_bindings.rs`
- Python package metadata: `pyproject.toml`
- Docs site content: `website/src/content/docs/**`
- CI quality gates: `.github/workflows/ci.yml`

## Non-Negotiable Alignment Rule

If a change affects user-visible behavior, update all affected surfaces in the same PR.

Examples of behavior changes:

- command/subcommand/flag/profile rename
- endpoint/path/query param change
- API function signature/return shape change
- source registry/count/metadata change
- telemetry/report schema change

## Cross-Surface Update Checklist

When changing **core behavior** (`openentropy-core`):

1. Update CLI usage/help and command docs if outputs/options changed.
2. Update Python bindings if public Rust behavior exposed there changed.
3. Update docs pages under `website/src/content/docs/**` and any canonical README snippets.
4. Update/extend tests in affected crates.

When changing **CLI** (`openentropy-cli`):

1. Keep clap definitions and docs examples in sync.
2. Verify documented commands/flags exist in `--help` output.
3. Update `website/src/content/docs/cli/index.md` and `website/src/content/docs/cli/reference.md`.

When changing **Python bindings** (`openentropy-python`):

1. Keep `#[pyfunction]`/`#[pymethods]` exports aligned with docs.
2. Keep health/report keys stable unless intentionally versioned.
3. Run parity checks (see Verification section).

When changing **HTTP server** (`openentropy-server`):

1. Keep endpoint paths and params aligned with docs and CLI examples.
2. Update integration/troubleshooting docs if behavior changes.

When changing **docs only**:

1. Validate every command/example against code, not assumptions.
2. Do not introduce commands/flags/functions that are not implemented.

## Drift Traps To Avoid

- Do not document commands that do not exist (for example, stale aliases).
- Do not use helper functions in examples that require undeclared dependencies.
- Do not hardcode counts (like source totals) in multiple places without verifying against code.
- Do not rename CLI flags in docs without corresponding clap changes.

## Verification Before Merge

Run relevant checks for your change; for cross-surface changes run all:

```bash
# Rust quality gates
cargo fmt --all -- --check
cargo clippy --workspace --exclude openentropy-python -- -D warnings
cargo test --workspace --exclude openentropy-python

# Python bindings smoke + parity
python -m venv .venv && source .venv/bin/activate
pip install maturin
maturin develop --release
python scripts/ci/check_python_source_parity.py

# Docs build
cd website && npm run build
```

Also verify CLI/docs parity quickly:

```bash
cargo run -p openentropy-cli -- --help
```

## Release/Version Hygiene

- Keep version changes synchronized with `CHANGELOG.md` format used by release workflow.
- Do not break tag-driven release assumptions in `.github/workflows/release.yml`.

## Commit/PR Guidance

- Prefer one logical change per PR.
- If behavior changed, include a short "cross-surface updates" note in PR description listing what was updated (CLI, Python, docs, tests).
