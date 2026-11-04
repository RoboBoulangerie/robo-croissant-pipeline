---
name: croissant-metadata-generator
version: 1.0.16
temperature: 0.1
description: Main agent for generating Croissant Metadata JSON-LD files.
mode: all
---

Before starting, refer to the `agents/shared/master-planner.md` file for a high-level overview of the agent's role and responsibilities.

# Objective

Generate a complete Croissant Metadata JSON-LD file with strong validation, deterministic output, and clear reporting of assumptions for a single configured knowledge source. Include "distribution" and "recordSet" metadata based on a
complete list of available files to download.

# Performance Metrics

Capture execution metrics throughout the run and include them in the final summary.

- Record a run-level start timestamp before Step 1 begins and a run-level end timestamp after Step 10 completes.
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

# Tooling

This agent may run under different coding-agent harnesses (OpenCode, Pi, Claude Code, ...). Rely only on capabilities they all share: running shell commands, reading files, and writing/editing files.

- Run every command shown in backticks in these instructions with your shell tool.
- To fetch web pages and check URLs, use a built-in web-fetch tool if one is available; otherwise use `curl` from the shell (for example `curl -fsSL '<url>'` for content, `curl -sSIL '<url>'` for headers/redirects, and `curl -fsSL -o '<path>' '<url>'` for downloads).
- Do not depend on subagents, MCP servers, or harness-specific tools.

# Other Instructions

- Do not source from github or other public repositories to resolve Knowledge Base Croissant Metadata files.

# Steps

## Step 1: check requirements

**Requirement**: 
  - Run the following command and if the exit code is not 0, terminate this program, but print the standard output: `nu ./bin/requirements.nu`

Be sure to inform the user why the agent failed to continue if a failure occurred.

## Step 2: resolve target knowledge source

Resolve two inputs independently: (1) the path to a knowledge bases TOML file (`RCP_KB_INPUT`) and (2) the entry name in that file (`RCP_KB_NAME`). Each input may be supplied in the user's prompt or through its environment variable. Use a value explicitly supplied in the prompt when both are present; otherwise use the corresponding environment variable. Do not default to a particular file or entry.

- If either input is missing or empty, report which input is needed and stop before initialization.
- Read the specified TOML file and find exactly one `knowledge_bases` entry whose `name` matches the specified entry name exactly (case-sensitive). If the file cannot be read or parsed, or there is not exactly one match, report the issue and stop.
- Read `metadata_urls` and `download_urls` from that entry as separate arrays of absolute HTTP(S) URLs. Report malformed or missing arrays or invalid URLs and stop; an empty array is valid and means no crawl roots for that category. Do not substitute the entry's `url` for either array.
- Treat that matched record as the only source to process.
- Do not iterate over all `knowledge_bases` entries.

## Step 3: initialization

- **`{name}`**
  - sourced from `name` from the `knowledge_bases` entry
- **`{run}`**
  - an integer (1, 2, 3, …) that increments with each new invocation; determine it by listing `./outputs` and
    taking one more than the highest existing integer subdirectory, or `1` if none exist
- **`{tmp_dir}`**
  - a temp directory that is created using the following command: `mktemp -d -p ./outputs/{name}/{run}` (run it after `./outputs/{name}/{run}` exists)

Create a directory structure following this format: `./outputs/{name}/{run}/{tmp_dir}`.  Do not create a temporary directory in `/tmp`.  All generated scripts and/or Agent generated code should be placed in `./outputs`.  Do not inspect or use previous `run` directories.  For example, if the current `run` is `3`, then `1` and `2` are off limits. 

## Step 4: Crawl and download files

Use **every** URL in the selected entry's `metadata_urls` as a crawl root for all top-level Croissant fields **except** `distribution` and `recordSet`. Use **every** URL in `download_urls` as a separate crawl root to discover and download the files for `distribution` and to inspect their schemas for `recordSet`. Follow relevant in-scope links from each root. Do not use the entry's `url` as a crawl root or as a fallback when either array is empty. Keep the source URL for each discovered value or file; a URL listed in both arrays may serve both purposes. Download relevant files into `./outputs` from Step 3. If either array is empty or a root cannot be crawled, document the corresponding evidence gap in `dct:provenance` rather than borrowing evidence from the other category.

There are two primary goals when downloading files from a web crawl:

1. satisfy the top-level metadata needed for a Croissant Metadata file, such as `name`, `description`, `keywords`, license, citation, and access conditions
2. retrieve the complete set of publicly available dataset-like and knowledge-graph-like downloadable assets that should be represented in `distribution` and used to derive `recordSet` metadata

### Crawl scope and traversal rules

- Treat every URL in each configured array as a discovery root, not as the only page to inspect. Explore all roots in both arrays, even when they are on different hosts.
- Recursively traverse all publicly accessible descendant pages and container pages that are in scope for that knowledge source.
- If a configured download root is a listing page, index page, catalog page, release page, graph registry page, dataset registry page, or directory-style page, continue traversing into its child pages until no new in-scope dataset or knowledge graph assets are discovered.
- When a root page contains multiple sibling graph, dataset, release, version, or export branches, traverse all of them, not just the first matching branch.
- Do not stop at one level of depth. Continue descending through nested paths such as version folders, `latest`, `current`, `release`, `releases`, `archive`, `downloads`, `files`, `data`, `dumps`, `exports`, `graphs`, `kg`, `knowledge-graph`, `ontology`, or similarly named subpaths when they appear relevant.
- Treat human-readable index pages and machine-generated directory listings as navigational containers to be explored, not as terminal pages.
- Stay within the same knowledge source scope for each root by default:
  - prefer relevant linked pages on the same host, including descendant paths under that root's path prefix
  - also allow direct download links on trusted linked download hosts or CDNs when those links are clearly presented by an in-scope source page as official downloadable assets
- Skip pages or files that require authentication, credentials, tokens, or other access restrictions.

### What to include

Include all publicly available downloadable assets that are clearly any of the following:

- datasets
- dataset releases
- data dumps
- exports
- graphs
- archives containing datasets
- knowledge graph releases
- graph serializations
- ontology or terminology releases
- machine-readable data bundles
- database snapshots
- structured documentation files that materially support dataset interpretation, such as PDFs describing schemas, releases, or usage

Do not restrict discovery to a narrow extension allowlist. Include files that are clearly dataset-like, graph-like, ontology-like, archival, or otherwise machine-readable release artifacts.

Files should preferentially include, but are not limited to, the following extensions:

- tsv
- tab
- txt
- csv
- json
- jsonl
- ndjson
- xml
- rdf
- owl
- obo
- ttl
- nt
- nq
- trig
- n3
- hdt
- graphml
- gml
- xgmml
- gpml
- sif
- gaf
- gpad
- gpi
- assoc
- parquet
- avro
- orc
- feather
- arrow
- sqlite
- db
- duckdb
- sql
- h5
- hdf5
- biom
- fasta
- fa
- fna
- fsa
- fastq
- gff
- gff3
- gtf
- bed
- vcf
- bcf
- sam
- bam
- maf
- gz
- bgz
- bz2
- xz
- zst
- zip
- tar
- tgz
- 7z
- pdf

Also include files whose URLs, anchor text, surrounding page text, or HTTP headers strongly indicate that they are downloadable datasets, graph releases, ontology exports, or release bundles even when the filename extension is missing, unconventional, or hidden behind query parameters.

### Discovery heuristics

While crawling, prioritize following links and pages whose URLs, titles, or nearby text suggest any of the following:

- download
- files
- data
- export
- details
- dump
- archive
- release
- releases
- version
- latest
- current
- graph
- graphs
- kg
- knowledge graph
- ontology
- rdf
- ttl
- obo
- owl
- api export
- bulk download
- snapshot
- artifacts

If a page appears to be a container for multiple graph or dataset families, enumerate each family and traverse each branch to its downloadable leaf assets.

### Download completeness requirement

The download set for Step 4 should be as complete as reasonably possible for the configured knowledge source's public data holdings discoverable from **all** `download_urls` roots and their in-scope linked pages.

For Croissant generation:

- include each discovered downloadable data or graph asset in `distribution`
- use the full discovered set, not a partial subset
- derive `recordSet` metadata where possible from the downloaded files themselves
- if a branch appears to contain relevant public data but cannot be fully traversed or downloaded, record the limitation and reason in `dct:provenance`

### Exclusions

- Exclude HTML pages from `distribution`, though they may still be fetched for metadata discovery.
- Exclude restricted, credential-gated, or explicitly unavailable files.
- Exclude irrelevant binaries or assets that are not dataset-like, graph-like, documentation-like, or otherwise useful for Croissant metadata generation.

## Step 5: generate croissant metadata file

Review the crawl evidence and files in `./outputs` and generate a file called `croissant.json` in the `./outputs` directory that is an as complete Croissant Metadata JSON file as possible. Derive top-level fields other than `distribution` and `recordSet` from the `metadata_urls` crawl; derive `distribution` and `recordSet` only from the complete set of files and schema evidence found through the `download_urls` crawl. Do not treat a file linked only from a metadata crawl as a distribution asset unless it is also discovered through a download root.

### Schema/Header Detection

Perform schema/header detection only for downloadable files whose resolved data format is in one of these groups:

- Strongest Schema Sources: `parquet`, `avro`, `orc`, `feather`, `arrow`, `sqlite`, `db`, `duckdb`, `sql`, `h5`, `hdf5`, `biom`, `graphml`, `xgmml`, `gpml`, `xml`, `owl`, and `obo`.
- Common Column or Record Headers: `csv`, `tsv`, `tab`, `txt`, `vcf`, `bcf`, `sam`, `bam`, `gff`, `gff3`, `gtf`, `gaf`, `gpad`, `gpi`, `fasta`, `fa`, `fna`, `fsa`, `fastq`, `jsonl`, `ndjson`, and `maf`.

Apply this allowlist to the actual inner or decompressed format, not merely the outer archive or compression extension. For an allowed file, attempt to extract its schema or header information to determine how a Croissant Metadata `recordSet` can be structured. Limit the number of lines sampled for determining datatypes to 5.

From the repository root, execute the parser commands directly for every supported local file:

- For `csv`, `tsv`, `tab`, `txt`, `sam`, `bam`, `vcf`, `bcf`, `parquet`, `avro`, `jsonl`, `ndjson`, and `arrow`, run `cargo run --quiet --bin parse_headers -- '<local-file-path>'`. Treat its JSON `columns` output as the ordered field names and datatypes, including generated `column_N` names when a delimited file has no detected header.  Use the output to determine the `recordSet` structure, field names, and datatypes.
- For `sqlite`, or a `db` file confirmed to be SQLite, run `nu ./bin/parse_sqlite_schema.nu '<local-file-path>'`. Treat its JSON output as the exact table names, field names, and declared datatypes.
- For `sql`, run `nu ./bin/parse_sql_schema.nu '<local-file-path>'`. Treat its JSON output as the exact table names, field names, and declared datatypes from the file's `CREATE TABLE` statements.  Use the output to determine the `recordSet` structure, field names, and datatypes.
- Only for `xml`, run `nu ./bin/parse_xml_schema.nu '<local-file-path>'`; do not run this parser for any other file type. Use the generated XSD schema output to create a schema-descriptive `cr:RecordSet` with the detected structure, exact field names, nesting, repetition, and datatypes. Set each field's `source` to a direct reference to the XML `cr:FileObject`, represented as `{ "@id": "<xml-file-object-id>" }`. Do not add `extract`, XPath, JSONPath, column, or regex selectors to these XML fields: Croissant 1.1 has no general XML selector, and these fields describe the parsed schema rather than an executable extraction mapping. State this limitation and the use of the generated XSD in `dct:provenance`; do not omit the XML `recordSet` merely because no executable XML selector is available.
- For `json`, generate a `cr:RecordSet` only when the file has a consistent array-of-objects or another clearly repeated record structure. Use exact object keys as field names, infer datatypes from no more than 5 records, and use valid JSONPath extraction expressions. Do not model one-off metadata objects as record sets or flatten inconsistent nested objects speculatively.
- For text RDF serializations (`rdf`, `ttl`, `trig`, `n3`, `nt`, and `nq`), a stable triple or quad statement model is sufficient: create fields for `subject`, `predicate`, and `object`, plus `graph` for named-graph formats when present. Prefix declarations are metadata, not fields. For RDF/XML, inspect XML structure as additional evidence but model RDF statements rather than XML syntax unless a separate application schema is explicitly supplied.
- For `gml`, use declared property names from the feature elements or an accompanying XSD when present; generate fields only when feature records and their properties are consistent. Treat geometry as a structured field only when its representation is clear.
- For `bed`, use an explicit header, track/browser metadata, or the standard BED column order (`chrom`, `chromStart`, `chromEnd`, then the documented optional BED fields). Generate only the columns actually present and do not invent names for non-standard extra columns without documented evidence.
- For `sif`, use the standard three-column structure `source`, `interaction`, and `target` when every sampled row conforms. For `assoc`, use an explicit header or documented format specification; do not infer fields merely from the extension because ASSOC has no single universal schema.
- If a supported file is compressed or contained in an archive, decompress or extract it into `./outputs` before invoking the corresponding schema/header detection command based on the file type.
- Use the parser-returned column names exactly.  Do not normalize, split, capitalize, or otherwise rewrite them.
- If a parser command fails, record the command error and limitation in `dct:provenance`, then apply the remaining detection rules in this section. Do not fabricate parser output.

Do not perform schema/header inference for `hdt` files. HDT is a compressed binary RDF representation whose dictionary and triple structure require specialized inspection not provided by this workflow. Represent it in `distribution`, but do not generate `recordSet` fields from guessed structure. For every format above, omit the `recordSet` and explain the limitation in `dct:provenance` when the required structural evidence is absent or inconsistent.

For all remaining columnar files that are not handled by the above parsers or excluded from detecting schema/header information, include the file among the Croissant Metadata 'distribution' entries and attempt to identify  appropriate 'recordSet' fields. 

Here some rules to follow when parsing a columnar file not handled by a pre-defined parser:
- do not assume the first non-empty line is the only source of schema or header information. Inspect the beginning of the file for commented metadata, preambles, or descriptive blocks that may declare column names, schema notes, datatypes, units, or header conventions before the tabular data begins.
- inspect leading comment lines and pre-header text before the main data rows
- treat commented schema lines as evidence when they clearly describe the columns, even if the actual header row is missing, abbreviated, or partially inconsistent
- consider common comment prefixes such as `#`, `//`, `;`, `--`, or similarly obvious file-level comment markers when they appear at the top of the file
- look for phrases such as `columns`, `fields`, `schema`, `header`, `format`, `delimiter`, `dictionary`, `data dictionary`, or similarly explicit schema hints
- if both top-of-file comments and an in-band header row are present, reconcile them and prefer the interpretation that is most explicit and internally consistent
- if an actual in-band header row is present in the downloadable file, preserve each header token exactly as it appears in that row unless the file itself explicitly documents that a different canonical machine-readable field name should be used
- if the top comments indicate there is no header row, use the documented schema from those comments to build the `recordSet` fields rather than inventing placeholder names
- record in `dct:provenance` when recordSet fields were derived from commented schema information, when comments conflict with the observed header row, or when the schema remains ambiguous after inspection
- if schema or header information is detected from the file contents or from top-of-file comments, preserve each detected column name exactly as written. Do not split, expand, title-case, normalize punctuation, infer word boundaries, or otherwise transform detected names.
- when downloadable file headers conflict with human-readable field lists, download-page labels, documentation tables, XML element labels, or prior assumptions, prefer the exact header strings found in the file for the Croissant `field.name` values and record the conflict in `dct:provenance`
- do not "improve readability" of detected headers. Specifically, do not insert commas, spaces, capitalization changes, or inferred word boundaries into observed header names, and do not replace a machine header with a nicer-looking documentation label when the machine header is available
- only synthesize fallback placeholder names or inferred labels when no reliable schema/header information is detected at all. If fallback names are necessary, record that fact and the reason in `dct:provenance`.
- Location-style columns need not be precise row-level coordinates, but should be semantically related to the location-oriented columns in the source data. 

If any of the values cannot be found or satisfied, provide a reason in the `dct:provenance` field in the croissant metadata file.

## Step 6: Generate JSON paths file

After generating the `./outputs` file, generate a JSON file called `persistent_fields.json` in the `./outputs` directory.   

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
      - a reference to the specific page or URL the information came from in the appropriate `metadata_urls` or `download_urls` crawl, not a local file path
    - `confidence` 
      - a score with a range between 0.0 and 1.0 reflecting how confident the agent is in reconciling the value from the actual URL.

Here is a sample output:
```
[
{ "path": "name", "value": "Comparative Toxicogenomics Database (CTD)", "url": "https://ctdbase.org/", "confidence": 0.97 },
{ "path": "keywords", "value": ["environmental chemicals", "genes", "diseases"], "url": "https://ctdbase.org/", "confidence": 0.32 }
]
```

## Step 7: Validation

### Run the MLCroissant Validation tool

Run the official validator tool using the following command: `.venv/bin/mlcroissant`.  The `mlcroissant` command expects a 'validate' subcommand, a '--jsonld' flag, and the generated found in `./outputs` file.

Here is an example usage of the `mlcroissant` command: `.venv/bin/mlcroissant validate --jsonld outputs/CTD/1/croissant.json`.  

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
       - re-fetch the relevant page reached from `metadata_urls` and confirm these are exact quotes, not paraphrases
     - `conditionsOfAccess`
       - re-fetch the terms page and confirm any quoted policy language is verbatim

For each field fetch the source URL, then find the relevant sentence or paragraph, and ask: "Does the Croissant value match the source text exactly, or has it been paraphrased, reformatted, or partially rewritten?" If there is any divergence, correct the Croissant value to match the source exactly.

## Step 8: Apply Fixes

### Missing Required Fields

Inspect every top-level field required by the Croissant specification and ensure that it is present and non-empty. Treat a missing property, `null`, an empty or whitespace-only string, or an empty array or object as missing. Populate required top-level metadata other than `distribution` and `recordSet` from authoritative pages reached through `metadata_urls` whenever possible, and do not invent source-specific values.

- `license`: Use the canonical URL for the license stated by the dataset source, such as `https://creativecommons.org/licenses/by/4.0/`. Verify that the selected URL identifies the exact license and version stated by the source. If no license can be found after checking the relevant homepage, terms, license, documentation, and citation pages reached through `metadata_urls`, set `license` to the literal string `Not Found` and explain the search limitation in `dct:provenance`. Do not infer a license from unrelated datasets, software repositories, or general website copyright text.
- `conformsTo`: Set the value to exactly `http://mlcommons.org/croissant/1.1`.
- `url`: Set the value to the canonical dataset homepage URL supported by the `metadata_urls` crawl. Prefer the dataset-specific landing page published by the authoritative source rather than a download URL, individual file URL, API endpoint, search result, repository, or general organization homepage. Follow redirects and store the final canonical URL when it remains the authoritative dataset homepage; do not copy the entry's `url` without evidence from `metadata_urls`.
- Re-check all other top-level required fields and populate any missing values with source-supported metadata. Preserve exact source text where the field validation rules require it, and record any unresolved required metadata in `dct:provenance`.
- Do not complete Step 8 while a required field remains empty. If no source-supported value can be established and this section does not define a fallback, mark the fix as failed and report the unresolved field rather than inventing a value.
- Re-run the official Croissant validator after repairing required fields and fix all resulting errors. Include the number of required fields checked, empty fields found, fields populated, `license` values set to `Not Found`, and additional validator runs or fix cycles in the Step 8 performance metrics.

### Fix Repeated contentUrl

Each top-level `distribution` entry must represent a distinct downloadable file or file set. A `contentUrl` used by a `cr:FileObject` must identify that specific file. Do not reuse a fallback, global download, archive, directory, landing page, or collection URL as the `contentUrl` for multiple `cr:FileObject` entries.

- Inspect all non-empty `contentUrl` values across the top-level `distribution` array and identify every URL repeated by more than one distribution entry.
- For each repeated URL, return to the source website and try to locate an authoritative, direct URL for each distinct file. Assign a URL to a `cr:FileObject` only when the source or a successfully resolved download request establishes that the URL identifies that specific file.
- Do not manufacture a per-file URL by appending a filename, changing path segments, guessing query parameters, or otherwise extrapolating from another URL unless the resulting URL is confirmed by the source and successfully resolves to the intended file.
- When the only verified URL identifies a top-level archive, directory, export endpoint, or other collection containing the files, replace the affected `cr:FileObject` entries with one `cr:FileSet` distribution whose `contentUrl` is that verified collection URL. Preserve applicable metadata on the `cr:FileSet` and describe in `dct:provenance` why individual file URLs were not represented.
- When distinct files are known but no authoritative URL for an individual file can be found, retain the file metadata only when it remains useful and valid, but omit `contentUrl` from that `cr:FileObject` rather than guessing or reusing a collection-level URL. Record the missing per-file URL and the discovery limitation in `dct:provenance`.
- After replacing, consolidating, or updating distribution entries, update every dependent `recordSet`, `field.source`, `containedIn`, `@id`, and other cross-reference so that it targets the surviving distribution entry and accurately describes whether the source is a `cr:FileObject` or `cr:FileSet`. Remove dangling references and do not claim file-level extraction metadata that cannot be supported by the surviving source.
- Repeat the duplicate scan until every remaining non-empty `distribution[n].contentUrl` is unique across the
  top-level `distribution` array.
- Re-run the official Croissant validator after these corrections and fix all resulting errors. Include the number of repeated URLs found, affected distribution entries, per-file URLs recovered, `cr:FileSet` entries created, `contentUrl` values omitted, references repaired, and additional validator runs or fix cycles in the Step 7 performance metrics.

### Add Missing Inner File Metadata From Archives

A `distribution` entry may represent a compressed archive such as `.zip`, or `.tgz` that contains one or more tabular or structured files, including TSV, CSV, JSON, JSONL, or similar formats. Do not represent only the downloadable parent archive when its inner files can be inspected and described. Represent relevant inner files in `distribution`, but perform schema/header inference and generate `recordSet` fields only for inner formats allowed by **Schema/Header Detection**. Note that a file ending in '.gz' does not mean it is an archive. This step is intended to only process archive files and not just compressed files (e.g. *.csv.gz and *.tsv.gz are not archives, whereas *.tar.gz, *.tgz, and *.zip are archives).   

- Inspect every downloaded archive and enumerate its inner files without modifying the original archive.
- Retain the parent archive as a `cr:FileObject` in the top-level `distribution` array. Give it a stable, unique `@id`, the archive's verified remote `contentUrl`, and the appropriate archive `encodingFormat`.
- Add a separate `cr:FileObject` to the top-level `distribution` array for each relevant inner file. Give every inner file its own stable, unique `@id`, exact archive-relative path or filename as `contentUrl`, appropriate `encodingFormat`, and a `containedIn` object whose `@id` references the parent archive's `@id`.
- Do not assign the parent archive's remote URL to an inner file. The inner file's `contentUrl` identifies the member within the archive, while `containedIn` identifies the downloadable parent archive.
- Derive inner filenames and paths from the archive contents. Do not guess an inner filename or create metadata for a member that cannot be verified in the downloaded archive.
- For each inner file whose resolved format is allowed by **Schema/Header Detection** and that supports records, generate a corresponding `cr:RecordSet`. Apply all rules from **Schema/Header Detection**, including inspection of comments and preambles for columnar files, preservation of exact detected column names, sampling no more than 5 lines or records for datatype detection, and provenance for ambiguous or inferred schema information. Do not generate inferred `recordSet` fields when the format-specific evidence required by that section is absent.
- Point every generated field source to the corresponding inner `cr:FileObject`, not the parent archive. For schema-descriptive XML fields, use the direct reference `source: { "@id": "<inner-xml-file-object-id>" }` without an extraction selector. For other supported formats, set `source.fileObject.@id` and preserve any extraction, column, or JSON path information needed to locate the field within that inner file.
- Verify that every inner file reference resolves to an existing distribution `@id`, every `containedIn.@id` resolves to the correct parent archive distribution, and no `RecordSet` field that was derived from an inner file points directly to the archive.
- During URL response validation, check the parent archive's remote `contentUrl` with HTTP. Validate an inner file's archive-relative `contentUrl` by confirming that the member exists in the downloaded archive; do not treat that relative member path as a standalone HTTP URL.
- Re-run the official Croissant validator after adding archive and inner-file metadata and fix all resulting errors. Include the number of archives inspected, inner files represented, `cr:RecordSet` entries generated, field sources repaired, and additional validator runs or fix cycles in the Step 7 performance metrics.

Example inner-file distribution:

```json
{
  "@id": "file_inner_tsv",
  "@type": "cr:FileObject",
  "contentUrl": "FileName.tsv",
  "encodingFormat": "text/tab-separated-values",
  "containedIn": { "@id": "<archive-distribution-id>" }
}
```

### Missing Source Field

Inspect every `cr:Field` in every `cr:RecordSet`. For executable field mappings, ensure that `source` maps to an actual column or value location in the referenced file. For schema-descriptive XML fields created under **Schema/Header Detection**, require a direct `source.@id` reference to the XML `cr:FileObject`, but do not require or invent an extraction selector.

- Resolve each `source.fileObject.@id`, or direct `source.@id` used by a schema-descriptive XML field, to exactly one `cr:FileObject` in the top-level `distribution` array. Treat missing, ambiguous, or dangling `@id` references as errors.
- Open the resolved file and apply all rules from **Schema/Header Detection** to determine its actual schema or columns. For XML, rerun the XML parser as needed and verify fields against its generated XSD. For an inner archive member, resolve `containedIn.@id`, open the parent archive, and inspect the exact member identified by the inner file's archive-relative `contentUrl`; do not inspect the parent archive as though it were the data file.
- For non-XML executable mappings, verify that `source.extract.column`, or the equivalent supported selector used by the field, identifies a value that actually exists in the resolved file. Match an observed header token exactly, including case, spacing, punctuation, and spelling. Do not normalize, expand, split, title-case, or otherwise rewrite a detected column name.
- For non-XML files, when a reliable documented schema is used because the file has no in-band header, verify the selector against that schema and record in `dct:provenance` that the column mapping came from documented or commented schema information.
- If a field points to the wrong file but its source column exists in another represented file, update `source.fileObject.@id` only when the downloaded data establishes that the field came from that file.
- If the file reference is correct but the column selector is wrong, replace it with the exact verified column name only when the intended mapping is unambiguous. Do not choose a similarly named column based only on semantic resemblance.
- If no actual column or value location supports a non-XML executable field, remove the unsupported `cr:Field` and any dependent references rather than inventing a column or retaining a false source mapping. Do not remove an XSD-supported schema-descriptive XML field solely because Croissant 1.1 lacks an XML extraction selector; verify its direct file reference and document the limitation in `dct:provenance` instead.
- For hierarchical or composite XML fields, verify every leaf `subField` against the generated XSD and use the same direct XML `source.@id` reference without an extraction selector. For other formats, verify every leaf `subField` against its actual source column. A structural parent field may group verified subfields when permitted by the Croissant schema, but it must not introduce an unsupported source claim.
- Repeat the check until every remaining field-level source reference resolves to an existing `cr:FileObject`, and every non-XML executable field also resolves to a verified column or value location in that file.
- Re-run the official Croissant validator after repairing source mappings and fix all resulting errors. Include the number of fields checked, dangling file references found, source file references corrected, column selectors corrected, unsupported fields removed, and additional validator runs or fix cycles in the Step 8 performance metrics.

### File Extenstion vs MIME Type Mismatch

Inspect every `cr:FileObject` in the top-level `distribution` array and verify that its `encodingFormat` agrees with the extension and actual format of the file identified by `contentUrl`.

- Compare extensions case-insensitively. For remote URLs, determine the extension from the URL path after ignoring query parameters and fragments. For files contained in an archive, use the inner file's archive-relative `contentUrl`.
- Add `encodingFormat` when it is missing and the file format can be reliably determined.
- When `encodingFormat` conflicts with a verified file extension, file signature, or inspected contents, replace it with the correct MIME type. Correct `encodingFormat`; do not rename the file or modify `contentUrl` merely to make the existing value appear consistent.
- Use these mappings when the corresponding file format is verified:
  - `.csv` -> `text/csv`
  - `.tsv` -> `text/tab-separated-values`
  - `.json` -> `application/json`
  - `.parquet` -> `application/parquet`
  - `.zip` -> `application/zip`
- For compound or compressed extensions, assign the parent archive or compressed file its compression MIME type and assign each represented inner file the MIME type of the inner file. Do not assign an archive's MIME type to its child `cr:FileObject` entries.
- If the extension is absent, misleading, or ambiguous, inspect the downloaded file's signature and contents and consult authoritative HTTP `Content-Type` metadata when available. Record unresolved ambiguity in `dct:provenance` rather than guessing.
- After correcting MIME type mismatches, re-run the official Croissant validator and fix all resulting errors. Include the number of `cr:FileObject` entries checked, missing `encodingFormat` values added, mismatches corrected, ambiguous formats, and additional validator runs or fix cycles in the Step 8 performance metrics.

### Fix contentUrl Values with Faulty Response Error

- Recursively inspect every scalar `contentUrl` leaf within every entry in the top-level `distribution` array.
- For each URL, make an HTTP request, follow redirects, and record the final HTTP status. A `HEAD` request may be used first, but if the server rejects or does not support `HEAD` (for example, with `405 Method Not Allowed`) or the result is otherwise inconclusive, retry with a minimal `GET` request before deciding the URL's status.
- If a `contentUrl` returns `404 Not Found`, remove its owning entry from `distribution`. If an entry contains more than one `contentUrl` leaf and any one of them returns `404`, remove that entire distribution entry. Also remove or update any `recordSet`, `field.source`, or other metadata references that depend on the removed distribution entry so the Croissant metadata does not contain dangling references.
- If a `contentUrl` returns `401 Unauthorized` or `403 Forbidden`, retain the distribution entry because the response may indicate an authentication or authorization requirement rather than a missing resource. Add or update the dataset-level `conditionsOfAccess` to state that the affected resource URL returned the observed status and may require authentication or authorization. Do not claim that authentication is definitely required unless the source explicitly confirms it.
- Do not treat redirects to a successful final response as failures. For statuses other than `401`, `403`, or `404`, retain the entry unless the response clearly proves that the resource is unavailable; record any uncertainty or access limitation in `dct:provenance`.
- After any removal or metadata update, re-run the official Croissant validator and fix all resulting errors. Include the number of `contentUrl` values checked, distribution entries removed, authentication-related responses, HTTP retries, and extra validator runs or fix cycles in the Step 7 performance metrics.

### Sanitize the Croissant Metadata JSON file

Run the following command: `python3 ./bin/json_sanitize.py 'outputs/{name}/{run}/croissant.json'`.  This should escape any control characters from the values across the entire croissant.json file.  Verify that this is true with the `jq empty outputs/{name}/{run}/croissant.json` command.  If errors were encountered, try to rectify them and then run `jq empty outputs/{name}/{run}/croissant.json` again.  Repeat until no error are reported.

### Add GeoCoordinates hierarchical fields when supported by the source columns

After sanitizing the Croissant Metadata JSON file, inspect every generated `recordSet` and its `field` entries to determine whether the source data contains location-oriented columns that can be combined into a Schema.org `GeoCoordinates` hierarchical field.

- Look for columns such as `country`, `state`, `province`, `city`, `town`, `region`, `area`, `latitude`, and `longitude`.
- Use relaxed column-name matching rather than exact token matching only. Normalize candidate column names by ignoring case, whitespace, punctuation, underscores, hyphens, pluralization, and mashed-together word boundaries when the meaning remains clear.
- Also consider obvious naming variants and concatenated forms when they clearly refer to the same concepts.
- When a column name clearly describes a location concept but does not exactly match a narrow geographic token, map it to the most appropriate explicit subfield label under the `GeoCoordinates` parent. Prefer labels such as `addressCountry`, `addressRegion`, `addressLocality`, `latitude`, and `longitude`, and use another clearly named location-oriented subfield only when those labels do not fit the source semantics.
- For example, a column such as `studycountries` may map to `addressCountry`; `stateorprovince`, `province_name`, or `adminregion` may map to `addressRegion`; `cityname`, `town`, `municipality`, or `locality` may map to `addressLocality`; and `lat`, `latitude_deg`, `lon`, or `lng` may map to `latitude` and `longitude`.
- Permit additive hierarchical groupings that use these broader location-style subfields when needed, including `addressCountry`, `addressRegion`, `addressLocality`, `latitude`, and `longitude`, so long as the mapping from the original column name to the chosen subfield is explicit and semantically justified.
- Only add the hierarchical field when the grouped columns belong to the same logical location in the same source table or file.
- Add a parent `Field` entry to that `RecordSet` with `dataType` set to Schema.org `GeoCoordinates`.
- Represent the contributing columns as `subField` entries beneath that parent field.
- Reuse the exact source column names for the contributing `subField` names when practical, but when a normalized mapping is clearer for Croissant or Schema.org interpretation, explicitly document the mapping to subfield labels such as `addressCountry`, `addressRegion`, `addressLocality`, `latitude`, or `longitude`.
- Map each contributing `subField` to the original column source so the hierarchical field is additive and does not replace the original flat fields.
- If the source column is broader, plural, or study-level rather than row-level, only use it when the resulting hierarchical field still reflects a coherent location concept; otherwise leave the flat field as-is and explain the limitation in `dct:provenance`.
- Do not reject a potential hierarchical field solely because a column is plural, study-level, multi-valued, comma-delimited, semicolon-delimited, or free-text if the column meaning is still clearly geographic and can be mapped explicitly. For example, `studycountries`, `stateorprovince`, and `citytownregionarea` may still justify an additive hierarchical grouping when they describe the same logical place context for each record, even if the values are broader or less normalized than ideal.
- If the location columns are geographically meaningful but imperfectly normalized, prefer adding the hierarchical parent field with conservative provenance notes over omitting it entirely. Reserve omission for cases where the candidate columns are genuinely ambiguous, refer to different location concepts, or cannot be mapped semantically to a coherent place model.
- Prefer creating the `GeoCoordinates` parent field when either:
  - `latitude` and `longitude` are present, or
  - a meaningful hierarchical place combination is present, such as `country` + `state`/`province` + `city`/`town`/`region`
- If the available columns are ambiguous, insufficiently related, or appear to describe different location concepts, do not force a `GeoCoordinates` field; instead, record the reason in `dct:provenance`. When in doubt, prefer semantic correctness over aggressive matching, but do not treat lack of row-level latitude/longitude precision by itself as sufficient reason to omit an otherwise coherent location hierarchy.

After adding any `GeoCoordinates` field:
- run the validator again and fix any newly introduced errors
- run `python3 ./bin/json_sanitize.py 'outputs/{name}/{run}/croissant.json'` again
- verify again with `jq empty outputs/{name}/{run}/croissant.json`
- update performance metrics with the number of `GeoCoordinates` parent fields added and any extra validation or sanitize cycles triggered by this sub-step

## Step 9: Cleanup

Remove all files in the `./outputs` directory.

## Step 10: Summarize

Write a summary of the agent's work to a text file called `summary.txt` in the `./outputs` directory.
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
