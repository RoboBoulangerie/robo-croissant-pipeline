# RoboCroissant Pipeline

RoboCroissant generates [Croissant 1.1](https://mlcommons.org/croissant/) JSON-LD metadata for biomedical knowledge bases. A coding agent (OpenCode, Pi, or Claude Code) crawls each source in `knowledge-bases.toml`, describes its downloadable files and (where supported) record schemas, and records source URLs and confidence for review. Nushell scripts can combine the results in a SQLite database; a Python validator reports data-quality issues.

## Setup

Run commands from the repository root. You will need:

- An agent harness configured with a working model/provider: [OpenCode](https://opencode.ai/docs/), [Pi](https://pi.dev), or [Claude Code](https://docs.claude.com/en/docs/claude-code). See [Choosing a harness](#choosing-a-harness).
- [Nushell](https://www.nushell.sh/) (`nu`) for the pipeline scripts and SQLite storage.
- [Rust and Cargo](https://rustup.rs/) for the `parse_headers` schema parser.
- Python 3 and [uv](https://docs.astral.sh/uv/) for the Croissant validator and the optional quality checks.
- `curl` for crawling (harnesses without a web-fetch tool use it), `jq` for the agent's JSON checks; `trang` if processing XML schemas.

For OpenCode, `opencode.sample.json` is a configuration template. Copy it to `opencode.json` and set a model and credentials for your provider. The local config file is gitignored.

```sh
cp opencode.sample.json opencode.json
uv venv
uv pip install mlcroissant requests
```

The agent's preflight checks for `.venv/bin/mlcroissant`. The Python quality validator also needs `requests` (installed above). Nushell's built-in SQLite support creates the database; the `sqlite3` command-line program is not required.

## Generate metadata

`knowledge-bases.toml` contains the source inventory. Its `name` values select a source (case-sensitive); `enabled = true` determines which sources the batch script and database builder process. A source site may require access or be unavailable even when enabled. The agent requires a TOML file path and an entry name; provide each via `RCP_KB_INPUT` and `RCP_KB_NAME` respectively, or in the prompt. Prompt values override environment values independently.

Generate metadata for one source:

```sh
RCP_KB_INPUT=knowledge-bases.toml RCP_KB_NAME=CTD nu ./bin/run_agent.nu croissant-metadata-generator
nu ./bin/run_agent.nu croissant-metadata-generator "Use knowledge-bases.toml; entry name: CTD" --harness pi
```

To run the agent once per enabled sources:

```sh
nu ./bin/run_main_agent.nu --harness pi
```

### Choosing a harness

The agent prompts live in `agents/` as plain Markdown. `bin/run_agent.nu <agent> [task]` runs one of them under the harness named by `--harness` (or `RCP_HARNESS`), defaulting to `opencode`. `--model` (or `RCP_MODEL`) is passed through in the harness's own format; without it the harness's default model is used. `--dry-run` prints the command without running it; run it with no agent name to list the available agents. `run_main_agent.nu` and `run_comparisons.nu` accept the same `--harness` and `--model` flags.

| Harness | Command run by `run_agent.nu` | Model setup |
| --- | --- | --- |
| `opencode` | `opencode run --agent <agent> <task>` | `opencode.json` (see above). `.opencode/agents/*.md` are symlinks into `agents/`, so `opencode run --agent croissant-metadata-generator "run"` also still works directly. |
| `pi` | `pi -p --no-session --append-system-prompt <prompt> <task>` | `/login` inside `pi`, or a custom provider (e.g. Azure) in `~/.pi/agent/models.json`; select it with `--model provider/id`. |
| `claude` | `claude -p --append-system-prompt <prompt> --permission-mode bypassPermissions <task>` | `claude` login or `ANTHROPIC_API_KEY`. |

For `pi` and `claude`, the launcher strips the YAML frontmatter from `agents/<agent>.md` and passes the rest inline to append to the harness's system prompt. Both run without permission prompts (Pi has none; Claude Code needs `bypassPermissions` to work non-interactively), so the agent can run arbitrary shell commands and downloads: run it in a container or sandbox you are comfortable with. The prompts only assume shell, file read, and file write tools; when no web-fetch tool exists the agent uses `curl`.

To support another harness, add a case to the `match $harness` blocks in `bin/run_agent.nu`.

Generation can download large datasets and take significant time and disk space. Each invocation creates a numbered directory under `outputs/<knowledge-base>/<run>/`. The main artifacts are:

```text
outputs/<KB>/1/
├── croissant.json          # Croissant JSON-LD: dataset, distributions, record sets, fields
├── persistent_fields.json  # metadata values with source URLs and confidence scores
└── summary.txt             # run summary and performance metrics
```

The agent may also put generated scripts and temporary downloaded files in that run directory. Its workflow validates `croissant.json` with MLCroissant; you can check a result manually with:

```sh
.venv/bin/mlcroissant validate --jsonld outputs/<KB>/1/croissant.json
```

For local schema inspection, the agent uses `cargo run --quiet --bin parse_headers -- <file>` for supported formats, plus the `bin/parse_*_schema.nu` scripts for SQL, SQLite, and XML.

## Build and check the database

```sh
nu ./bin/build_db.nu
```

This **replaces** `robo_croissant.db` with metadata and evidence from enabled sources with files in `outputs/`. You can point it at another output directory with `nu ./bin/build_db.nu --outputs_dir ./outputs-10-01`. For each source, it picks the most recently modified `croissant.json` and `persistent_fields.json` independently; check that they belong to the same run before importing if multiple runs exist.

The database has `knowledge_bases` (metadata), `kb_links` (values, evidence, confidence, review state), and tables for runs and validation issues. 

To export one metadata document from the database, run this in Nushell:

```nu
open robo_croissant.db | get knowledge_bases | where name == '<KB.name>' | get croissant_metadata | to json | save -f output.json
```

Some [previously published Croissant files](https://roboboulangerie.github.io/robo-croissant-pipeline/) are also available on GitHub Pages.
