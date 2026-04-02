# AGENTS.md

Guidance for coding agents working in `robo-croissant-pipeline`.

## 1) Repository Snapshot

- Primary language: Nushell (`.nu` scripts).
- Main entrypoint: `main.nu`.
- Runtime config: `config.toml`.
- Main output artifact: `robo_croissant.db` (SQLite exported by Nushell `stor`).
- Containerized execution is supported via `Dockerfile`.
- Current scope is pipeline execution, crawling, extraction, and JSON assembly.

## 2) Rule Files (Cursor/Copilot)

- Checked for `.cursorrules`: not present.
- Checked for `.cursor/rules/`: not present.
- Checked for `.github/copilot-instructions.md`: not present.
- No repository-specific Cursor/Copilot policy files exist at this time.

## 3) Environment and Dependencies

- Rust toolchain is required (for installing CLI tools via Cargo).
- Nushell is required to run the pipeline.
- AIChat CLI is required (`aichat` command is used directly by `main.nu`).
- Spider CLI is required (`spider` command is used for crawling).
- `tidy` and `htmlq` are used by the crawler cleanup step; ensure both exist in PATH.
- SQLite file is generated locally; do not commit database outputs unless explicitly asked.

Install baseline tools (host):

```bash
cargo install nu aichat spider_cli
```

## 4) Build / Lint / Test Commands

There is no traditional compile/build step in this repo today; treat pipeline execution as the main integration run.

### Core commands

- Run full pipeline:

```bash
nu main.nu
```

- Expected primary output:

```bash
ls robo_croissant.db
```

- Parse/smoke check script loadability (fast sanity check):

```bash
nu -c 'source main.nu; "ok"'
```

### Lint/format status

- No dedicated formatter config or lint config is present in-repo.
- No CI workflow directory is present (`.github/workflows/` missing).
- Use conservative manual style checks and keep edits consistent with existing Nushell idioms.

### Test status

- No formal unit/integration test suite is currently present.
- No per-test runner exists yet (no `tests/` directory, no configured framework files found).

### “Single test” guidance (closest equivalent)

Because there is no native test harness, use a targeted single-source smoke run:

1. In `config.toml`, keep exactly one enabled `[[knowledge_sources]]` entry.
2. Run:

```bash
nu main.nu
```

3. Verify `robo_croissant.db` was produced and contains expected tables/records.

If you need repeatable single-test behavior, add a small Nushell test harness script in a future PR.

## 5) Code Style Guidelines

These conventions reflect current repository patterns and should be preserved.

### Imports / module loading

- Nushell does not use Python/JS-style imports in current files.
- Load reusable code with `source <file.nu>` when splitting logic across files.
- Keep module boundaries simple and explicit; avoid hidden side effects at load time.

### Formatting and layout

- Use 4-space indentation in function bodies and control blocks.
- Keep one logical operation per line when practical.
- Use blank lines to separate phases (crawl, extract, transform, persist).
- Prefer readable pipelines over dense one-liners.

### Naming conventions

- Functions: `snake_case` (`crawl_knowledge_source`, `parse_json_with_repair`).
- Variables: `snake_case` and descriptive (`source_blacklist`, `mapping_data`).
- Table names and keys: lowercase snake_case.
- Temporary directories: explicit names like `tmp_dir`, `croissant_spec_tmp_dir`.

### Types and data shapes

- Add Nushell parameter types where known (`string`, `int`).
- Keep JSON data as structured values as long as possible; parse early.
- Validate LLM output by parsing with `from json` and repair only on failure.
- Preserve stable field names in records inserted into `stor` tables.

### Error handling

- Wrap external command boundaries in `try { ... } catch {|e| ... }`.
- Fail soft for per-source failures when feasible; continue processing other sources.
- Print actionable errors (include context like source name/step).

### External command usage

- Quote or interpolate values safely in command arguments.
- Guard optional flags (as done for blacklist handling) instead of passing empties.
- Keep crawler depth and blacklist logic explicit and auditable.

### Data and persistence

- Recreate/reset `stor` tables intentionally at pipeline start.
- Keep schema definitions centralized near startup in `main`.
- Export final SQLite artifact once at the end of the run.

### Config handling

- Treat `config.toml` as the source of truth for prompts and sources.
- Prefer additive config edits; do not silently remove commented source templates.
- Use `%name%` prompt substitution consistently with current convention.

### Comments and documentation

- Keep comments concise and only for non-obvious behavior.
- Update `README.md` when command or dependency expectations change.

## 6) Change Management for Agents

- Make minimal, focused edits; avoid broad rewrites unless requested.
- Preserve existing CLI interfaces and file names unless migration is requested.
- If adding tests, document exact run commands in `README.md` and this file.
- If adding lint/format tools, include config files and one canonical command each.

## 7) Quick Pre-PR Checklist

- Script loads: `nu -c 'source main.nu; "ok"'`.
- Targeted run completed for at least one source.
- `robo_croissant.db` generation verified locally.
- No secrets committed (`.env` stays ignored).
- Docs updated when behavior/commands changed.
