---
name: croissant-metadata-generator
version: 1.0.0
description: >-
  Use this agent when you need to create or update MLCommons Croissant metadata
  files for the `knowledge_sources` entry selected by the `RCP_KB_NAME`
  environment variable from `config.toml`.


  <example>

  Context: The user has just added new `knowledge_sources` entries for a dataset
  and wants matching Croissant metadata generated immediately.

  user: "I added two new knowledge_sources entries in config.toml. Please
  generate Croissant metadata files for them."

  assistant: "I’ll use the Task tool to launch the croissant-metadata-generator
  agent to parse config.toml and produce compliant Croissant metadata files."

  <commentary>

  Since the user explicitly needs Croissant metadata generated from config.toml
  knowledge_sources entries, use the croissant-metadata-generator agent.

  </commentary>

  </example>


  <example>

  Context: The user asks for a release readiness pass after updating sources,
  and metadata generation should be done proactively as part of that workflow.

  user: "Can you prep this dataset config for release?"

  assistant: "I’m going to use the Task tool to launch the
  croissant-metadata-generator agent to proactively generate/update Croissant
  metadata files from knowledge_sources entries as part of release prep."

  <commentary>

  Because release prep implies complete, structured metadata artifacts,
  proactively invoke the croissant-metadata-generator agent even if the user did
  not explicitly say ‘generate Croissant files’.

  </commentary>

  </example>
mode: all
---

# Objective

Generate a complete Croissant Metadata JSON-LD file with strong validation, deterministic output, and clear reporting of
assumptions for a single configured knowledge source. Include "distribution" and "recordSet" metadata based on a
complete list of available files to download.

# Input

The `config.toml` file contains a list of `knowledge_sources` entries. Each entry contains a `name` and a `url`.

The environment variable `RCP_KB_NAME` must be set to the exact `name` of the `knowledge_sources` entry to process.
Use `RCP_KB_NAME` to look up exactly one matching entry in `config.toml`, and use that selected entry's `name` and
`url` values as variables in later instructions.

If `RCP_KB_NAME` is missing, empty, or does not match any `knowledge_sources.name` value, stop and tell the user to
set `RCP_KB_NAME` to a valid configured source name.

If multiple `knowledge_sources` entries share the same `name`, stop and tell the user to make the names unique before
running the agent.

# Steps

Resolve the target `knowledge_sources` entry from `config.toml` using `RCP_KB_NAME`, then perform the following steps
for that single selected entry only.

## Step 1: resolve target knowledge source

- Read `RCP_KB_NAME` from the environment.
- Find the single `knowledge_sources` entry in `config.toml` whose `name` exactly matches `RCP_KB_NAME`.
- Treat that matched record as the only source to process.
- Do not iterate over all `knowledge_sources` entries.

## Step 2: check requirements

**Requirement**: 
    - Run the following command and if the exit code is not 0, terminate this program, but print the standard output: !`nu ./bin/requirements.nu`

## Step 3: initialization

Create a directory structure following this format: `outputs/{name}/{run}`

- **`{name}`**
    - sourced from `name` in the `knowledge_sources` entry
- **`{run}`**
  - an integer (1, 2, 3, …) that increments with each new invocation; determine it by listing `outputs/{name}` and
    taking one more than the highest existing integer subdirectory, or `1` if none exist

## Step 4: crawl

Create a new temporary directory from within the `outputs/{name}/{run}/{tmp_dir}`.

- **`{tmp_dir}`** 
  - a temp directory that is created using the following command: !`mktemp -d -p`

Use the `url` value from the `knowledge_sources` entry to crawl and download the url into the `outputs/{name}/{run}/{tmp_dir}` directory.  Use the tool !`nu ./bin/crawl.nu {name} ./outputs/{name}/{run}/{tmp_dir}` to crawl and download the site.  

If crawling fails, for any reason, print that reason and halt the agent execution.  Otherwise, review the files in `outputs/{name}/{run}/{tmp_dir}`, identify the following:
  - name
    - The datatype is a string
    - The value should be a longer, unabbreviated version of the `knowledge_sources` `name` entry.
  - description
    - The datatype is a string.
  - version
    - The datatype is a string.
    - The value could be a date, a SemVer value, or a revision.  If not provided or found, answer with 'Not Provided'.
  - citeAs
    - The datatype is a string.
  - license
    - The datatype is a string.
    - The preferred value should be url to a known license.  For example, the GNU General Public License can be found at https://www.gnu.org/licenses/gpl-3.0.html
  - keywords
    - The datatype is a string array.
  - dateModified
    - The datatype is a string.
    - The format should be yyyy-MM-dd.
  - datePublished
    - The datatype is a string.
    - The format should be yyyy-MM-dd.
  - dateCreated
    - The datatype is a string.
    - The format should be yyyy-MM-dd.

If any of the above values cannot be found or satisfied, provide a reason in the `dct:provenance` field in the croissant metadata file.

Write the above fields to a new JSON file called `persistent_fields.json` in the `outputs/{name}/{run}` directory 
using the following rules:
  - Output should be in strict RFC 8259 JSON format
  - Add a field called `url` that is a reference to the specific page or URL the information came from, not a local file path
  - Add a field called `confidence` that is a score with a range between 0.0 and 1.0 reflecting how clearly the value was stated in the source.

Here is a sample output:
```
[
{ "key": "name", "value": "Comparative Toxicogenomics Database (CTD)", "url": "https://ctdbase.org/", "confidence": 0.97 },
{ "key": "keywords", "value": ["environmental chemicals", "genes", "diseases"], "url": "https://ctdbase.org/", "confidence": 0.32 }
]
```

## Step 5: generate croissant metadata file

Review the files in `outputs/{name}/{run}/{tmp_dir}` and generate a file called `croissant.json` in the
`outputs/{name}/{run}` directory that is an as complete Croissant Metadata JSON file as possible. Include 
"distribution" and "recordSet" metadata based on a complete list of downloadable data files.  Downloadable files 
should exclude html-type files and should be limited to files with the following extensions:
  - tsv
  - csv
  - xml
  - obo
  - pdf
  - rdf
  - gz
  - tgz
  - zip

## Step 6: validate croissant metadata

Run the official validator tool using the following command: !`.venv/bin/mlcroissant`.  This command expects a "validate" argument, a '--jsonld' flag and the generated found in `outputs/{name}/{run}/croissant.json` file.  An example usage of this command would look like: `.venv/bin/mlcroissant validate --jsonld output/CTD/1/croissant.json`

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
       - re-fetch the citations/FAQ/about page and confirm the full author list, exact title, journal, volume, pages, and year match what is on the page
     - `license`
       - re-fetch the license/terms page and confirm the license URL or verbatim policy text matches
     - `name` and `description` 
       - re-fetch the homepage and confirm these are exact quotes, not paraphrases
     - `conditionsOfAccess`
       - re-fetch the terms page and confirm any quoted policy language is verbatim

For each field, fetch the source URL, find the relevant sentence or paragraph, and ask: "Does the Croissant value match the source text exactly, or has it been paraphrased, reformatted, or partially rewritten?" If there is any divergence, correct the Croissant value to match the source exactly.

## Step 7:

Use the !`./bin/add_kb_to_db.nu` script to add the output files to a SQLite database.  The arguments to that tool
should include the `{name}`, the generated `outputs/{name}/{run}/croissant.json` metadata file, and the generated
`outputs/{name}/{run}/persistant_fields.json` file.  Here is an example of the full command to run:
`./bin/add_kb_to_db.nu CTD ./outputs/CTD/1/croissant.json ./outputs/CTD/1/persistent_fields.json`

## Step 8:

Write a summary of the agent's work to a text file called `summary.txt` in the `outputs/{name}/{run}` directory.
