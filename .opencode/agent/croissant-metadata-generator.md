---
name: croissant-metadata-generator
version: 1.0.0
description: >-
  Use this agent when you need to create or update MLCommons Croissant metadata
  files using `knowledge_sources` entries found in the `config.toml` file.


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

Review the Croissant Format Specification from https://docs.mlcommons.org/croissant/docs/croissant-spec-1.1.html. Then,
generate a complete Croissant Metadata JSON-LD file with strong validation, deterministic output, and clear reporting of
assumptions. Include "distribution" and "recordSet" metadata based on a complete list of available files to download.

# Input

The `config.toml` file contains a list of `knowledge_sources` entries. Each entry contains a 'name' and a 'url'.
The 'name' and 'url' values will be used as variables in later instructions.

# Steps

For each entry of `knowledge_sources`, perform the following steps.

## Step 1: initialization

Create a directory structure like the following:

```
outputs/{run}/{name}/
```

- **`{run}`** - an integer (1, 2, 3, …) that increments with each new invocation; determine it by listing `outputs/` and
  taking one more than the highest existing integer subdirectory, or `1` if none exist
- **`{name}`** - `name` is derived from the `knowledge_sources` entry

## Step 2:

Create a new temp directory from within the `outputs/{run}/{name}/{tmp_dir}`.

- **`{tmp_dir}`** - a temp directory that is created using the following command: !`mktemp -d -p`

Use the `url` value from the `knowledge_sources` entry to crawl and download the url into that new temp directory.  Use 
the tool !`nu ./bin/crawl.nu {name} ./outputs/{run}/{name}/{tmp_dir}` to crawl and download the site.  

Review the files in `outputs/{run}/{name}/{tmp_dir}`, identify the following:
  - name: string
  - description: string
  - version: string
  - citation: string
  - license: string
  - keywords: array of strings
  - dateModified: string
  - datePublished: string
  - dateCreated: string

Write the identified fields to a new JSON file called `persistent_fields.json` in the `outputs/{run}/{name}/` directory 
using the following rules:
  - Output should be in strict RFC 8259 JSON format
  - Output should include the url from where the answer is sourced and not a local file system path
  - Answer directly and without using Markdown

Here is an sample output:
```
[
{ "key": "sample_name_1", "value": "sample_value_1", "url": "sample_url_1" },
{ "key": "sample_name_2", "value": ["sample_value_2a", "sample_value_2b"], "url": "sample_url_2" }
]
```

## Step 3:

Review the files in `outputs/{run}/{name}/{tmp_dir}` and generate a file called `croissant.json` in the
`outputs/{run}/{name}/` directory that is an as complete Croissant Metadata JSON file as possible. Include 
"distribution" and "recordSet" metadata based on a complete list of available files to download.

## Step 4:

Use the !`./bin/add_kb_to_db.nu` script to add the output files to a SQLite database.  The arguments to that tool 
should include the `{name}`, the generated `outputs/{run}/{name}/croissant.json` metadata file, and the generated 
`outputs/{run}/{name}/persistant_fields.json` file.  Here is an example of the full command to run:
`./bin/add_kb_to_db.nu CTD ./outputs/1/CTD/croissant.json ./outputs/1/CTD/persistent_fields.json`

## Step 5:

