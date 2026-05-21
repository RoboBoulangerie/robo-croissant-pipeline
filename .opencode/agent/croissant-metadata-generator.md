---
name: croissant-metadata-generator
version: 1.0.1
temperature: 0.1
description: Main agent for generating Croissant Metadata JSON-LD files.
mode: all
---

Before starting, refer to the `.opencode/agent/subagent/master-planner.md` file for a high-level overview of the agent's role and responsibilities.

# Objective

Generate a complete Croissant Metadata JSON-LD file with strong validation, deterministic output, and clear reporting of
assumptions for a single configured knowledge source. Include "distribution" and "recordSet" metadata based on a
complete list of available files to download.

# Input

The `knowledge_bases.toml` file contains a list of `` entries. Each entry contains a `name` and a `url`.

The environment variable `RCP_KB_NAME` must be set to the exact `name` of the `knowledge_bases` entry to process.
Use `RCP_KB_NAME` to look up exactly one matching entry in `knowledge_bases.toml`, and use that selected entry's `name` and
`url` values as variables in later instructions.

If `RCP_KB_NAME` is missing or empty or does not match any `knowledge_bases.name` value, stop the agent and tell the user to set `RCP_KB_NAME` to a valid configured source name.

If multiple `knowledge_bases` entries share the same `name`, stop and tell the user to make the names unique before running the agent.

# Steps

Resolve the target `knowledge_bases` entry from `knowledge_bases.toml` using `RCP_KB_NAME`, then perform the following steps
for that single selected entry only.

## Step 1: resolve target knowledge source

- Read `RCP_KB_NAME` from the environment.
- Find the single `knowledge_bases` entry in `knowledge_bases.toml` whose `name` exactly matches `RCP_KB_NAME`.
- Treat that matched record as the only source to process.
- Do not iterate over all `knowledge_bases` entries.

## Step 2: check requirements

**Requirement**: 
    - Run the following command and if the exit code is not 0, terminate this program, but print the standard output: !`nu ./bin/requirements.nu`

## Step 3: initialization

Create a directory structure following this format: `outputs/{name}/{run}/{tmp_dir}`.

- **`{name}`**
  - sourced from `name` in the `knowledge_bases` entries
- **`{run}`**
  - an integer (1, 2, 3, …) that increments with each new invocation; determine it by listing `outputs/{name}` and
    taking one more than the highest existing integer subdirectory, or `1` if none exist
- **`{tmp_dir}`**
  - a temp directory that is created using the following command: !`mktemp -d -p`

## Step 4: Download and parse html

Download the pages using the `url` value from the `knowledge_bases.name` entry and write the HTML output to the `outputs/{name}/{run}/{tmp_dir}` directory.  Be sure to include all pagination, writing those files to an incrementing output.  For example, if a file is called `datasets.html`, but has JavaScript enabled pagination indicated by a "Next" and/or "Previous" buttons, then write the paginated pages out page by page called 'datasets_page_001.html', 'datasets_page_002.html', and so on. 

Review the files in `outputs/{name}/{run}/{tmp_dir}`, identify the following:
  - name
    - The datatype is a string
    - The value should be a longer, unabbreviated version of the `knowledge_bases` `name` entry.
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
  - rdf
  - gz
  - tgz
  - zip
  - pdf
  - parquet

For each downloadable file, attempt to extract header information of that file to satisfy how a Croissant Metadata "recordSet" can be structured.  For example, if a '.tsv' file has a header, sample the first 10 lines to determine datatype and name of the column to build out the "recordSet" fields.  

## Step 6: Generate JSON paths file

After generating the `outputs/{name}/{run}/{tmp_dir}/croissant.json` file, generate a TSV file called `croissant_paths.tsv` in the `outputs/{name}/{run}` directory.   

Use these rules for `croissant_paths.tsv`:
  - The first column, named 'path', in this file is the leaf-only JSON path for every element in the `croissant.json` file, with one path per line.  
    - derive the paths from the final `croissant.json` exactly as written
    - include only leaf paths whose values are scalars such as strings, numbers, booleans, or null
    - do not include container-only paths for objects or arrays
    - use `[index]` for array positions
    - use `key` for simple identifier keys and `"key"` when the key contains characters that require quoting
    - do not apply any additional filtering unless a later instruction explicitly requires it
    - exclude any linked element, those with a key starting with '@'
    - exclude the 'dct:provenance' element
  - The second column, named 'url', must be the URL where the value from this path is derived.  
  - The third column, named 'confidence', must be a confidence score.  The value ranges from 0.0 to 1.0 and indicates how confident the agent is in reconciling the value from the actual URL.  

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

## Step 9: Summarize

Write a summary of the agent's work to a text file called `summary.txt` in the `outputs/{name}/{run}` directory.
Include the generated artifact paths for `croissant.json`, `croissant_paths.tsv`, and `persistent_fields.json`.
