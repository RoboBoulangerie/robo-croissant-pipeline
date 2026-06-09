---
name: croissant-metadata-generator
version: 1.0.2
temperature: 0.1
description: Main agent for generating Croissant Metadata JSON-LD files.
mode: all
---

Before starting, refer to the `.opencode/agent/subagent/master-planner.md` file for a high-level overview of the agent's role and responsibilities.

# Objective

Generate a complete Croissant Metadata JSON-LD file with strong validation, deterministic output, and clear reporting of
assumptions for a single configured knowledge source. Include "distribution" and "recordSet" metadata based on a
complete list of available files to download.

# Performance Metrics

Capture execution metrics throughout the run and include them in the final summary.

- Record a run-level start timestamp before Step 1 begins and a run-level end timestamp after Step 9 completes.
- For every numbered step, record:
  - step name
  - start timestamp
  - end timestamp
  - duration in seconds with at least millisecond precision when available
  - outcome: `success`, `warning`, or `failed`
- For every step that changes the filesystem, compare the filesystem state immediately before and after that step and record:
  - number of files created
  - number of files modified
  - number of files deleted
- Count file changes across all files touched by the agent during the run, including generated outputs, downloaded files,
  temporary files, and database files.
- Also capture useful step-specific counts when available, such as:
  - number of files downloaded in Step 4
  - total bytes downloaded in Step 4
  - number of data files included in `distribution`
  - number of `recordSet` entries generated
  - number of validator runs and fix cycles in Step 7
- Maintain cumulative totals for the full run:
  - total duration
  - total files created
  - total files modified
  - total files deleted

If a step fails, still record its partial metrics before exiting or retrying.

# Other Instructions

- Do not source from github or other public repositories to resolve Knowledge Base Croissant Metadata files.

# Steps

## Step 1: check requirements

**Requirement**: 
  - Run the following command and if the exit code is not 0, terminate this program, but print the standard output: !`nu ./bin/requirements.nu`

## Step 2: resolve target knowledge source

Resolve the target `knowledge_bases` entry from `knowledge_bases.toml` using environment varialbe `RCP_KB_NAME`, then perform the following steps using only that selected entry.

- Read `RCP_KB_NAME` from the environment.
- Find the single `knowledge_bases` entry in `knowledge_bases.toml` whose `name` exactly matches `RCP_KB_NAME`.
- Treat that matched record as the only source to process.
- Do not iterate over all `knowledge_bases` entries.

## Step 3: initialization

- **`{name}`**
  - sourced from `name` in the `knowledge_bases` entries
- **`{run}`**
  - an integer (1, 2, 3, …) that increments with each new invocation; determine it by listing `outputs/{name}` and
    taking one more than the highest existing integer subdirectory, or `1` if none exist
- **`{tmp_dir}`**
  - a temp directory that is created using the following command: !`mktemp -d -p`

Create a directory structure following this format: `outputs/{name}/{run}/{tmp_dir}`.  Do not create a temporary directory in `/tmp`.  All generated scripts and/or Agent generated code should be placed in `outputs/{name}/{run}/bin`.

## Step 4: Crawl and download files

Using the `knowledge_bases` entry value from the `url` field and the `outputs/{name}/{run}/{tmp_dir}` value from the previous step, crawl the source website and download all files into the `outputs/{name}/{run}/{tmp_dir}` directory.  There are two primary goals when downloading files from a web crawl.  The first is to satisfy all the top-level metadata from a Croissant Metadata file, such as `name`, `description`, `keywords`, and such.  The second is to ensure that dataset-based downloadable files are retrieved.    

Rules to follow while searching for dataset-based files to download:
- Exclude html-type files
- Include all publicly available files
- Skip files that may be restricted or requiring credentials 
- Files should have the following extensions:
  - tsv
  - json
  - jsonl
  - csv
  - xml
  - obo
  - rdf
  - gz
  - tgz
  - zip
  - pdf
  - parquet

## Step 5: generate croissant metadata file

Review the files in `outputs/{name}/{run}/{tmp_dir}` and generate a file called `croissant.json` in the `outputs/{name}/{run}` directory that is an as complete Croissant Metadata JSON file as possible. Include "distribution" and "recordSet" metadata based on a complete list of downloadable data files.  

For each downloadable file, attempt to extract header information of that file to satisfy how a Croissant Metadata "recordSet" can be structured.  For example, if a '.tsv' file has a header, sample the first 5 lines to determine the column name and datatype to build out the "recordSet" fields.  

If any of the values cannot be found or satisfied, provide a reason in the `dct:provenance` field in the croissant metadata file.

## Step 6: Generate JSON paths file

After generating the `outputs/{name}/{run}/{tmp_dir}/croissant.json` file, generate a TSV file called `persistent_fields.json` in the `outputs/{name}/{run}` directory.   

Use the following rules to produce the `persistent_fields.json` file:
  - Output should be in strict RFC 8259 JSON format
  - Each entry in the output should be a JSON object with the following fields:
    - `path` 
      - derive the paths from the final `croissant.json` exactly as written
      - include only leaf paths whose values are scalars such as strings, numbers, booleans, or null
      - do not include container-only paths for objects or arrays
      - use `[index]` for array positions
      - use `key` for simple identifier keys and `"key"` when the key contains characters that require quoting
      - do not apply any additional filtering unless a later instruction explicitly requires it
      - exclude any linked element, those with a key starting with '@'
      - exclude the 'dct:provenance' element
    - `value`
      - a string or array of strings derived from the final `croissant.json` exactly as written
    - `url`
      - a reference to the specific page or URL the information came from, not a local file path
    - `confidence` 
      - a score with a range between 0.0 and 1.0 reflecting how confident the agent is in reconciling the value from the actual URL.

Here is a sample output:
```
[
{ "path": "name", "value": "Comparative Toxicogenomics Database (CTD)", "url": "https://ctdbase.org/", "confidence": 0.97 },
{ "path": "keywords", "value": ["environmental chemicals", "genes", "diseases"], "url": "https://ctdbase.org/", "confidence": 0.32 }
]
```

## Step 7: validate croissant metadata

Run the official validator tool using the following command: !`.venv/bin/mlcroissant`.  The `mlcroissant` command expects a 'validate' subcommand, a '--jsonld' flag, and the generated found in `outputs/{name}/{run}/croissant.json` file.

Here is an example usage of the `mlcroissant` command: `.venv/bin/mlcroissant validate --jsonld output/CTD/1/croissant.json`.  

Do not include the `--help` flag to infer the arguments for the `mlcroissant` command.

Review the standard output from the `mlcroissant` command and perform the following:
 - **On errors**
   - Fix every `E` (error) line before delivering the output. 
   - Do not suppress or work around validator errors — they indicate genuine non-conformance. 
   - Re-run the validator after each fix cycle until it reports zero errors.
 - **On warnings**
   - Warnings (`W`) about missing recommended fields (`datePublished`, `version`, etc.) should be ignored.
 - **Content validation** 
   - After the structural validator passes, re-fetch the source page for each of these fields and compare the Croissant value character-by-character against the live page text:
     - `citeAs` 
       - re-fetch the citations, FAQ, or about pages to confirm the full author list, exact title, journal, volume, pages, and year match what is on the page
     - `license`
       - re-fetch the license/terms page and confirm the license URL or verbatim policy text matches
     - `name` and `description` 
       - re-fetch the homepage and confirm these are exact quotes, not paraphrases
     - `conditionsOfAccess`
       - re-fetch the terms page and confirm any quoted policy language is verbatim

For each field fetch the source URL, then find the relevant sentence or paragraph, and ask: "Does the Croissant value match the source text exactly, or has it been paraphrased, reformatted, or partially rewritten?" If there is any divergence, correct the Croissant value to match the source exactly.

## Step 8: Add output to robo_croissant.db

Use the !`./bin/add_kb_to_db.nu` script to add the output files to a SQLite database.  The arguments to that tool
should include the `{name}`, the `outputs/{name}/{run}/croissant.json` metadata file, and the `outputs/{name}/{run}/persistent_fields.json` file.  

Here is an example of the full command to run: 
`./bin/add_kb_to_db.nu CTD ./outputs/CTD/1/croissant.json ./outputs/CTD/1/persistent_fields.json`

## Step 9: Cleanup

Remove all files in the `outputs/{name}/{run}/{tmp_dir}` directory.

## Step 10: Summarize

Write a summary of the agent's work to a text file called `summary.txt` in the `outputs/{name}/{run}` directory.
Include the generated artifact paths for `croissant.json`, and `persistent_fields.json`.
The summary must include a dedicated `Performance Metrics` section with:

- run start timestamp
- run end timestamp
- total duration
- total files created
- total files modified
- total files deleted
- a per-step table or structured list covering every numbered step with:
  - step number and step name
  - duration
  - outcome
  - files created, modified, and deleted for that step
  - any relevant step-specific counts such as downloaded file count, bytes downloaded, validator runs, or fix cycles
- a final artifact list with the generated paths for `croissant.json`, `persistent_fields.json`, and `summary.txt`
