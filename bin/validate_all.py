"""
Run programmatic auto-validation against a SQLite DB or Croissant JSON file.

What this script does:
  1. Finds fields that pass vocabulary or URL checks.
  2. For database input, writes passing fields and detected data-quality issues
     back to the database so the dashboard can display them at /issues.
  3. Writes a human+LLM-readable feedback report to ./reports/ explaining
     what went wrong and how to fix it.

Usage:
    python3 bin/validate_all.py
    python3 bin/validate_all.py --db robo_croissant.db
    python3 bin/validate_all.py --json croissant.json --links persistent_fields.json
    python3 bin/validate_all.py --dry-run          # preview, no writes
    python3 bin/validate_all.py --kb "BRCA Exchange"  # single KB
    python3 bin/validate_all.py --reset            # clear all auto_reviewed first

Requires:
    pip install requests
"""

import argparse
import datetime
import json
import os
import re
import sqlite3
import sys
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
    import requests
except ImportError:
    print("ERROR: 'requests' package is required.  Install with:  pip install requests")
    sys.exit(1)

DB_PATH = "../robo-croissant-pipeline/robo_croissant.db"
REPORTS_DIR = "reports"
HTTP_WORKERS = 20
HTTP_TIMEOUT = 15

# ── Croissant / schema.org controlled vocabularies (mirrors main.rs) ─────────
VALID_TYPES = {
    "sc:Dataset", "cr:FileObject", "cr:FileSet", "cr:RecordSet", "cr:Field",
    "sc:Organization", "sc:Person", "sc:CreativeWork",
}
VALID_DATA_TYPES = {
    "sc:Text", "sc:Integer", "sc:Float", "sc:Boolean",
    "sc:Date", "sc:Time", "sc:DateTime", "sc:URL",
    "sc:Number", "cr:CellSet",
}
VALID_DIST_TYPES = {"cr:FileObject", "cr:FileSet"}
CROISSANT_CONFORMS_TO = "http://mlcommons.org/croissant/1.1"

EXT_TO_MIMES: dict[str, set[str]] = {
    ".csv": {"text/csv"},
    ".tsv": {"text/tab-separated-values", "text/tsv"},
    ".json": {"application/json"},
    ".jsonl": {"application/jsonlines", "application/x-ndjson"},
    ".ndjson": {"application/jsonlines", "application/x-ndjson"},
    ".parquet": {"application/parquet", "application/vnd.apache.parquet"},
    ".zip": {"application/zip"},
}

_DIST_ID_RE = re.compile(r"^distribution\[\d+\]\.@id$")
_DIST_PFX_RE = re.compile(r"^(distribution\[\d+\])\.@id$")
_DIST_TYPE_RE = re.compile(r"^distribution\[\d+\]\.@type$")
_DIST_URL_RE = re.compile(r"^(distribution\[\d+\])\.contentUrl$")
_FIELD_SRC_RE = re.compile(r"source\.file(?:Object|Set)\.@id$")
_FIELD_COL_RE = re.compile(r"^(recordSet\[\d+\]\.field\[\d+\])\.source\.extract\.column$")
_FIELD_NM_RE = re.compile(r"^(recordSet\[\d+\]\.field\[\d+\])\.name$")
_FIELD_SRCID_RE = re.compile(r"^(recordSet\[\d+\]\.field\[\d+\])\.source\.file(?:Object|Set)\.@id$")
_PLACEHOLDER_RE = re.compile(r"^\[.+\]$|^<.+>$")
_PLACEHOLDER_WORDS = {
    "todo", "tbd", "n/a", "none", "unknown", "null",
    "example", "placeholder", "insert here", "your text here",
}
# Matches URL path segments that indicate a terms/privacy page rather than a license
_LICENSE_TERMS_RE = re.compile(
    r"[/\-_](terms|privacy|tos|conditions)(?:[/\-_.\?#]|$)"
    r"|/legal(?:[/.\?#]|$)",
    re.IGNORECASE,
)
_MD5_PATH_RE = re.compile(r"^distribution\[\d+\]\.md5$")
_SHA256_PATH_RE = re.compile(r"^distribution\[\d+\]\.sha256$")
_HEX_RE = re.compile(r"^[0-9a-fA-F]+$")

# ── Human-readable metadata for each issue type ───────────────────────────────
ISSUE_META: dict[str, dict] = {
    "repeated_content_url": {
        "label": "Repeated contentUrl",
        "severity": "error",
        "why": (
            "In the Croissant specification, each `distribution` represents a distinct file. "
            "Every `distribution[n].contentUrl` must be a unique URL pointing to that specific "
            "file. Repeating the same URL across many distributions means individual files "
            "cannot be located or validated — and signals the LLM used a fallback/global "
            "download link instead of finding per-file URLs."
        ),
        "fix": (
            "Assign each distribution its own `contentUrl`. "
            "If only a top-level archive or directory URL is known, create a single "
            "`cr:FileSet` distribution pointing there instead of many `cr:FileObject` "
            "distributions sharing the same URL. "
            "If individual file URLs are genuinely unknown, omit `contentUrl` rather than guessing."
        ),
    },
    "duplicate_distribution_id": {
        "label": "Duplicate distribution @id",
        "severity": "error",
        "why": (
            "The `@id` field is a unique identifier for a Croissant object. "
            "If two distributions share the same `@id`, consumers cannot distinguish them, "
            "and any `recordSet` field that references the distribution by `@id` is ambiguous."
        ),
        "fix": (
            "Generate a unique `@id` for each distribution. "
            "A reliable pattern is to derive it from the file name or path, "
            "e.g. `dist_<slugified-filename>`. Append a short hash if the name is not unique."
        ),
    },
    "relative_content_url": {
        "label": "Relative contentUrl",
        "severity": "warning",
        "why": (
            "A `contentUrl` like `output/release/diff/added.tsv` is a relative path "
            "inside a repository or local filesystem. External consumers of the Croissant "
            "file cannot resolve it to an actual download. "
            "The LLM most likely copied internal project paths from a README or Makefile "
            "rather than constructing fully-qualified URLs."
        ),
        "fix": (
            "Always use absolute URLs starting with `http://`, `https://`, or `ftp://`. "
            "Construct the full URL from the known hosting base, e.g.:\n"
            "  `output/release/diff/added.tsv`\n"
            "  → `https://github.com/ORG/REPO/raw/main/output/release/diff/added.tsv`\n"
            "If a canonical download URL is not publicly available, omit `contentUrl`."
        ),
    },
    "wrong_distribution_type": {
        "label": "Wrong distribution @type",
        "severity": "warning",
        "why": (
            "A `distribution` entry must have `@type` of either `cr:FileObject` (a single "
            "file) or `cr:FileSet` (a collection of files, e.g. a glob pattern). "
            "Using `sc:Dataset` or any other type at the distribution level is invalid "
            "and will cause schema validation failures."
        ),
        "fix": (
            "Use `cr:FileObject` for a single downloadable file, or `cr:FileSet` for a "
            "pattern-matched collection. The top-level dataset itself uses `sc:Dataset`."
        ),
    },
    "broken_source_reference": {
        "label": "Broken source cross-reference",
        "severity": "error",
        "why": (
            "A `recordSet[n].field[m].source.fileObject.@id` value references a distribution "
            "by its `@id`, but no distribution with that `@id` exists in this dataset. "
            "This means the field's data source cannot be resolved, breaking the "
            "lineage chain that Croissant relies on for data loading."
        ),
        "fix": (
            "The value of `source.fileObject.@id` must exactly match the `@id` of one of "
            "the distributions defined in the same Croissant file. "
            "Check for typos, case differences, or stale references left over from an "
            "earlier version of the metadata."
        ),
    },
    "format_extension_mismatch": {
        "label": "Format/extension mismatch",
        "severity": "warning",
        "why": (
            "The `distribution[n].encodingFormat` value does not match the MIME type implied "
            "by the file extension in `contentUrl`. For example, a URL ending in `.csv` "
            "should have `encodingFormat: text/csv`. A mismatch can mislead data-loading "
            "tools that rely on the declared format."
        ),
        "fix": (
            "Ensure `encodingFormat` uses the standard MIME type for the file's extension:\n"
            "  .csv  → text/csv\n"
            "  .tsv  → text/tab-separated-values\n"
            "  .json → application/json\n"
            "  .parquet → application/parquet\n"
            "  .zip  → application/zip"
        ),
    },
    "placeholder_value": {
        "label": "Placeholder value",
        "severity": "warning",
        "why": (
            "A field value appears to be a template placeholder (e.g. `[name]`, `<title>`, "
            "`TODO`, `N/A`) rather than real content. The LLM generated a scaffolding "
            "structure but left these fields unfilled."
        ),
        "fix": (
            "Replace placeholder values with the actual information. "
            "If the information is genuinely unknown, omit the field entirely "
            "rather than leaving a placeholder that could be mistaken for real data."
        ),
    },
    "url_unreachable": {
        "label": "URL unreachable",
        "severity": "info",
        "why": (
            "An HTTP HEAD request to the `contentUrl` failed with a connection error or "
            "timeout. This may be a transient network issue, the server may block HEAD "
            "requests, or the URL may no longer exist."
        ),
        "fix": (
            "Verify the URL manually. If it resolves in a browser but not via HEAD, "
            "the server may require GET or authentication. "
            "If the file has moved, update `contentUrl` to the new location."
        ),
    },
    "url_error_response": {
        "label": "URL error response",
        "severity": "warning",
        "why": (
            "An HTTP HEAD request to the `contentUrl` returned a non-success status code "
            "(e.g. 404 Not Found, 403 Forbidden). The file is not accessible at the "
            "declared URL."
        ),
        "fix": (
            "A 404 means the file does not exist at this URL — update `contentUrl` to "
            "the correct location or remove the distribution. "
            "A 403/401 may mean the resource requires authentication; "
            "if so, note this in `conditionsOfAccess` at the dataset level."
        ),
    },
    "license_is_terms_page": {
        "label": "License URL is a terms page",
        "severity": "warning",
        "why": (
            "The `license` field should identify the legal terms under which the dataset "
            "may be used for downstream purposes (e.g. training, redistribution, commercial "
            "use). The URL provided points to a terms-of-service, terms-of-use, privacy, or "
            "legal page, which governs *platform access*, not *data reuse rights*. "
            "ML tools consuming Croissant metadata cannot determine reuse rights from a "
            "terms-of-service URL."
        ),
        "fix": (
            "Replace with the specific data license URL for this dataset. "
            "Use a standard, machine-readable license URI where possible:\n"
            "  CC BY 4.0  → https://creativecommons.org/licenses/by/4.0/\n"
            "  CC BY 3.0  → https://creativecommons.org/licenses/by/3.0/\n"
            "  CC0        → https://creativecommons.org/publicdomain/zero/1.0/\n"
            "  ODC-By     → https://opendatacommons.org/licenses/by/1-0/\n"
            "If the data requires registration or has non-standard terms, omit `license` "
            "and describe the conditions in `conditionsOfAccess` instead."
        ),
    },
    "invalid_checksum_format": {
        "label": "Invalid checksum format",
        "severity": "warning",
        "why": (
            "The value of an `md5` or `sha256` field does not match the expected hex "
            "encoding. An MD5 digest is exactly 32 lowercase hex characters; a SHA-256 "
            "digest is exactly 64 lowercase hex characters. A malformed checksum cannot "
            "be used by tools to verify file integrity after download."
        ),
        "fix": (
            "Replace the value with the correct hex-encoded digest for the file.\n"
            "  MD5    → 32 hex chars, e.g. d8e8fca2dc0f896fd7cb4cb0031ba249\n"
            "  SHA256 → 64 hex chars, e.g. "
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n"
            "If the checksum was computed with a different algorithm, use the matching "
            "field name (sha1, sha512, etc.) rather than forcing it into sha256 or md5."
        ),
    },
    "placeholder_field_name": {
        "label": "Placeholder field name",
        "severity": "warning",
        "why": (
            "A field name like `column_1`, `column_2`, etc. is a generic placeholder, "
            "not an actual column name from the data file. This occurs when the LLM "
            "pipeline could not read the file's header row (e.g. the file was behind "
            "authentication, compressed in an unsupported format, or the download failed)."
        ),
        "fix": (
            "Inspect the actual file to obtain its real column names and replace the "
            "placeholder names. If the file is inaccessible, remove the recordSet or "
            "mark the field names as unknown rather than using `column_N` placeholders."
        ),
    },
    "missing_required_field": {
        "label": "Missing required field",
        "severity": "error",
        "why": (
            "A field required by the Croissant 1.1 specification or Schema.org Dataset "
            "vocabulary is absent from the metadata. Missing `license` means consumers "
            "cannot determine reuse rights. Missing `conformsTo` means tools cannot verify "
            "which Croissant version the file targets. Missing `url` or `description` makes "
            "the dataset undiscoverable."
        ),
        "fix": (
            "Add the missing field with an appropriate value. For `license`, use a "
            "canonical license URL (e.g. `https://creativecommons.org/licenses/by/4.0/`). "
            "For `conformsTo`, use `http://mlcommons.org/croissant/1.1`. "
            "For `url`, provide the canonical dataset homepage URL."
        ),
    },
    "missing_field_source": {
        "label": "Field missing source",
        "severity": "error",
        "why": (
            "A `cr:Field` has no `source` property. In Croissant, `source` is what connects "
            "a field definition to the actual column in a file via `source.fileObject.@id` "
            "and `source.extract.column`. Without it the field exists in the schema but cannot "
            "be used to load data — consumers have no way to know which file or column provides "
            "this field's values."
        ),
        "fix": (
            "Add a `source` to each field. The typical pattern is:\n"
            '  "source": {\n'
            '    "fileObject": { "@id": "<distribution-id>" },\n'
            '    "extract": { "column": "<exact column name from the file header>" }\n'
            "  }\n"
            "For a field from a file inside a zip, first define the inner file as a "
            "FileObject with `containedIn`, then reference that inner FileObject here."
        ),
    },
    "inline_data_rows": {
        "label": "Inline data rows (should be examples)",
        "severity": "warning",
        "why": (
            "A `recordSet` contains a `data` key with embedded rows. In Croissant 1.1, "
            "`data` means these rows ARE the complete records for this recordSet — it implies "
            "the recordSet is self-contained with no external file source. For large datasets "
            "that have distribution files, a few sample rows belong in `examples`, not `data`. "
            "Using `data` misleads consumers into thinking the dataset only contains these rows."
        ),
        "fix": (
            "Rename `data` to `examples`. The `examples` key is the correct place for "
            "illustrative sample rows in a recordSet that loads its actual data from a file. "
            "Reserve `data` only for truly inline datasets (e.g. a small lookup table "
            "that has no external file and whose complete contents are embedded in the metadata)."
        ),
    },
    "zip_without_inner_file": {
        "label": "Compressed archive missing inner file",
        "severity": "warning",
        "why": (
            "A distribution is a compressed archive (`.zip`, `.gz`, `.tgz`) containing a "
            "tabular file (TSV, CSV, JSON), but no other `cr:FileObject` is defined with "
            "`containedIn` pointing to this archive. Without an inner-file FileObject, "
            "there is no way for a recordSet field to declare its `source.fileObject.@id` "
            "at the correct granularity — the field can only point to the archive, not the "
            "actual tabular file inside it."
        ),
        "fix": (
            "Define a second `cr:FileObject` for the inner file, setting `containedIn` to "
            "the archive's `@id`. Then update `recordSet.field[*].source.fileObject.@id` to "
            "reference the inner file. Example:\n"
            '  { "@id": "file_inner_tsv",\n'
            '    "@type": "cr:FileObject",\n'
            '    "contentUrl": "FileName.tsv",\n'
            '    "encodingFormat": "text/tab-separated-values",\n'
            '    "containedIn": { "@id": "<archive-distribution-id>" } }'
        ),
    },
}


# ── HTTP helpers ──────────────────────────────────────────────────────────────

def _head(url: str):
    try:
        resp = requests.head(
            url, timeout=HTTP_TIMEOUT, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1"},
        )
        return url, resp
    except Exception:
        return url, None


def _get_csv_header(url: str, encoding_format: str = "") -> list[str] | None:
    """Fetch the first line of a plain text CSV/TSV file and return column names."""
    url_lc = url.lower()
    try:
        resp = requests.get(
            url, stream=True, timeout=30, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1", "Range": "bytes=0-8191"},
        )
        if not (resp.ok or resp.status_code == 206):
            return None
        chunk = b""
        for block in resp.iter_content(8192):
            chunk = block
            break
        resp.close()
        text = chunk.decode("utf-8", errors="replace")
        first_line = text.split("\n")[0].rstrip("\r")
        fmt_lc = encoding_format.lower()
        is_tsv = ("tab" in fmt_lc or "tsv" in fmt_lc or ".tsv" in url_lc or "\t" in first_line)
        if is_tsv:
            return [c.strip().strip('"') for c in first_line.split("\t")]
        import csv as _csv
        return next(_csv.reader([first_line]), [])
    except Exception:
        return None


def _get_gz_columns(url: str, encoding_format: str = "") -> list[str] | None:
    """Download, decompress a gzip stream, and return first-line CSV/TSV column names."""
    import gzip
    import io
    try:
        resp = requests.get(
            url, stream=True, timeout=60, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1"},
        )
        if not resp.ok:
            return None
        buffer = b""
        for chunk in resp.iter_content(65536):
            buffer += chunk
            if len(buffer) >= 131072:  # 128 KB is plenty for a header row
                break
        resp.close()
        if not buffer:
            return None
        with gzip.open(io.BytesIO(buffer), "rb") as gz:
            first_bytes = gz.read(8192)
        text = first_bytes.decode("utf-8", errors="replace")
        first_line = text.split("\n")[0].rstrip("\r")
        if not first_line:
            return None
        fmt_lc = encoding_format.lower()
        url_lc = url.lower()
        is_tsv = ("tab" in fmt_lc or "tsv" in fmt_lc or ".tsv" in url_lc or "\t" in first_line)
        if is_tsv:
            return [c.strip().strip('"') for c in first_line.split("\t")]
        import csv as _csv
        return next(_csv.reader([first_line]), [])
    except Exception:
        return None


def _get_jsonl_columns(url: str) -> list[str] | None:
    """Read the first line of a JSONL file and return the object's keys."""
    import json as _json
    try:
        resp = requests.get(
            url, stream=True, timeout=30, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1", "Range": "bytes=0-8191"},
        )
        if not (resp.ok or resp.status_code == 206):
            return None
        chunk = b""
        for block in resp.iter_content(8192):
            chunk = block
            break
        resp.close()
        first_line = chunk.decode("utf-8", errors="replace").split("\n")[0].rstrip("\r")
        obj = _json.loads(first_line)
        return list(obj.keys()) if isinstance(obj, dict) else None
    except Exception:
        return None


def _get_json_columns(url: str) -> list[str] | None:
    """Read a JSON file and return field names from the first object found."""
    import json as _json
    try:
        resp = requests.get(
            url, stream=True, timeout=30, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1", "Range": "bytes=0-8191"},
        )
        if not (resp.ok or resp.status_code == 206):
            return None
        chunk = b""
        for block in resp.iter_content(8192):
            chunk = block
            break
        resp.close()
        text = chunk.decode("utf-8", errors="replace")
        # Try first line as JSONL
        try:
            obj = _json.loads(text.split("\n")[0])
            if isinstance(obj, dict):
                return list(obj.keys())
        except Exception:
            pass
        # Try as JSON array of objects or a top-level object
        try:
            data = _json.loads(text)
            if isinstance(data, list) and data and isinstance(data[0], dict):
                return list(data[0].keys())
            if isinstance(data, dict):
                return list(data.keys())
        except Exception:
            pass
        return None
    except Exception:
        return None


def _get_parquet_columns(url: str) -> list[str] | None:
    """Read a Parquet file schema and return column names. Requires pyarrow."""
    try:
        import io
        import pyarrow.parquet as pq
        resp = requests.get(
            url, timeout=120, allow_redirects=True,
            headers={"User-Agent": "robo-croissant-validator/0.1"},
        )
        if not resp.ok:
            return None
        schema = pq.read_schema(io.BytesIO(resp.content))
        return list(schema.names)
    except ImportError:
        return None  # pyarrow not installed
    except Exception:
        return None


def _fetch_file_columns(url: str, fmt: str = "") -> list[str] | None:
    """Route to the appropriate column-fetching function based on URL extension."""
    url_lc = url.lower().split("?")[0]
    if url_lc.endswith(".gz"):
        return _get_gz_columns(url, fmt)
    if url_lc.endswith((".jsonl", ".ndjson")):
        return _get_jsonl_columns(url)
    if url_lc.endswith(".json"):
        return _get_json_columns(url)
    if url_lc.endswith(".parquet"):
        return _get_parquet_columns(url)
    return _get_csv_header(url, fmt)


# ── Data quality flag checks ──────────────────────────────────────────────────

def flag_issues(links: list[dict], url_results: dict) -> list[dict]:
    issues = []
    link_map = {l["path"]: l for l in links}
    dist_ids = {l["value"] for l in links if _DIST_ID_RE.match(l["path"])}

    def add(path, value, issue_type, detail):
        issues.append({"path": path, "value": str(value)[:200],
                       "issue_type": issue_type, "detail": detail})

    # 1. Repeated contentUrl across distributions
    url_counts = Counter(
        l["value"] for l in links if _DIST_URL_RE.match(l["path"]) and l["value"]
    )
    reported_urls: set[str] = set()
    for l in links:
        if _DIST_URL_RE.match(l["path"]) and l["value"]:
            n = url_counts[l["value"]]
            if n > 1 and l["value"] not in reported_urls:
                reported_urls.add(l["value"])
                add(l["path"], l["value"], "repeated_content_url",
                    f"Same contentUrl shared by {n} distributions — likely LLM fallback URL")

    # 2. Duplicate @id within distributions
    dist_id_counts = Counter(
        l["value"] for l in links if _DIST_ID_RE.match(l["path"]) and l["value"]
    )
    reported_ids: set[str] = set()
    for l in links:
        if _DIST_ID_RE.match(l["path"]) and l["value"]:
            if dist_id_counts[l["value"]] > 1 and l["value"] not in reported_ids:
                reported_ids.add(l["value"])
                add(l["path"], l["value"], "duplicate_distribution_id",
                    f"@id used by {dist_id_counts[l['value']]} distributions — must be unique")

    # 3. Relative paths in contentUrl
    for l in links:
        if _DIST_URL_RE.match(l["path"]) and l["value"]:
            if not l["value"].startswith(("http://", "https://", "ftp://")):
                add(l["path"], l["value"], "relative_content_url",
                    "contentUrl is a relative path, not an absolute URL")

    # 4. Wrong @type for distribution context
    for l in links:
        if _DIST_TYPE_RE.match(l["path"]) and l["value"] is not None and l["value"] not in VALID_DIST_TYPES:
            add(l["path"], l["value"], "wrong_distribution_type",
                f"distribution @type must be cr:FileObject or cr:FileSet, got '{l['value']}'")

    # 5. Broken recordSet field source cross-reference
    for l in links:
        if _FIELD_SRC_RE.search(l["path"]) and l["value"]:
            if l["value"] not in dist_ids:
                add(l["path"], l["value"], "broken_source_reference",
                    f"source.fileObject.@id '{l['value']}' does not match any distribution @id")

    # 6. File extension / encodingFormat mismatch
    for l in links:
        m = _DIST_URL_RE.match(l["path"])
        if m and l["value"]:
            fmt_path = f"{m.group(1)}.encodingFormat"
            if fmt_path in link_map:
                declared = link_map[fmt_path]["value"]
                url_path = l["value"].split("?")[0].lower()
                for ext, ok_mimes in EXT_TO_MIMES.items():
                    if url_path.endswith(ext) and declared not in ok_mimes:
                        add(fmt_path, declared, "format_extension_mismatch",
                            f"URL ends in '{ext}' but encodingFormat is '{declared}' "
                            f"(expected one of: {', '.join(sorted(ok_mimes))})")
                        break

    # 7. Placeholder text
    import json as _json
    for l in links:
        if l["value"] is None:
            continue
        v = l["value"].strip()
        try:
            parsed = _json.loads(v)
            if isinstance(parsed, (list, dict)):
                continue
        except (ValueError, TypeError):
            pass
        if _PLACEHOLDER_RE.match(v) or v.lower() in _PLACEHOLDER_WORDS:
            add(l["path"], l["value"], "placeholder_value",
                "Value looks like a placeholder or LLM-generated default")

    # 8. License URL pointing to a terms/privacy/legal page instead of an actual license
    for l in links:
        if l["path"] in ("license", "license.url") and l["value"]:
            v = l["value"]
            if v.startswith(("http://", "https://")) and _LICENSE_TERMS_RE.search(v):
                add(l["path"], v, "license_is_terms_page",
                    "URL points to a terms-of-service or legal page, not a data license. "
                    "Replace with a standard license URI (e.g. a Creative Commons URL) or "
                    "move platform access conditions to conditionsOfAccess.")

    # 9. Checksum format validation (md5 = 32 hex, sha256 = 64 hex)
    for l in links:
        v = l["value"] or ""
        if not v:
            continue
        if _MD5_PATH_RE.match(l["path"]):
            if not (len(v) == 32 and _HEX_RE.match(v)):
                add(l["path"], v, "invalid_checksum_format",
                    f"md5 must be exactly 32 hex characters (got {len(v)})")
        elif _SHA256_PATH_RE.match(l["path"]):
            if not (len(v) == 64 and _HEX_RE.match(v)):
                add(l["path"], v, "invalid_checksum_format",
                    f"sha256 must be exactly 64 hex characters (got {len(v)})")

    # 10. Unreachable / erroring absolute URLs
    for l in links:
        if _DIST_URL_RE.match(l["path"]) and l["value"]:
            url = l["value"]
            if not url.startswith(("http://", "https://", "ftp://")):
                continue
            resp = url_results.get(url)
            if resp is None:
                add(l["path"], url, "url_unreachable",
                    "HEAD request failed (connection error or timeout)")
            elif not (resp.ok or resp.is_redirect):
                add(l["path"], url, "url_error_response",
                    f"HEAD returned HTTP {resp.status_code}")

    return issues


# ── Per-KB validation ─────────────────────────────────────────────────────────

def validate_kb(links: list[dict]) -> tuple[set[str], list[dict]]:
    link_map = {l["path"]: l for l in links}
    dist_ids = {l["value"] for l in links if _DIST_ID_RE.match(l["path"])}
    auto_paths: set[str] = set()

    for l in links:
        path, value, last = l["path"], l["value"], l["path"].split(".")[-1]
        if last == "@type" and value in VALID_TYPES:
            auto_paths.add(path)
        elif last == "dataType" and value in VALID_DATA_TYPES:
            auto_paths.add(path)
        elif path == "conformsTo" and value == CROISSANT_CONFORMS_TO:
            auto_paths.add(path)
        elif last == "@id":
            if _FIELD_SRC_RE.search(path):
                if value in dist_ids:
                    auto_paths.add(path)
            elif value:
                auto_paths.add(path)

    url_tasks = [
        (l["path"], m.group(1), l["value"])
        for l in links
        if (m := _DIST_URL_RE.match(l["path"])) and l["value"]
           and l["value"].startswith(("http://", "https://", "ftp://"))
    ]

    url_results: dict = {}
    if url_tasks:
        unique_urls = {url for _, _, url in url_tasks}
        with ThreadPoolExecutor(max_workers=HTTP_WORKERS) as pool:
            futures = {pool.submit(_head, url): url for url in unique_urls}
            for future in as_completed(futures):
                url, resp = future.result()
                url_results[url] = resp

        for path, prefix, url in url_tasks:
            resp = url_results.get(url)
            if resp is None or not (resp.ok or resp.is_redirect):
                continue
            auto_paths.add(path)
            url_filename = url.split("/")[-1].split("?")[0]
            name_path = f"{prefix}.name"
            if url_filename and name_path in link_map:
                if link_map[name_path]["value"] == url_filename:
                    auto_paths.add(name_path)
            fmt_path = f"{prefix}.encodingFormat"
            if fmt_path in link_map:
                ct = resp.headers.get("Content-Type", "").split(";")[0].strip()
                if ct and link_map[fmt_path]["value"] == ct:
                    auto_paths.add(fmt_path)
            size_path = f"{prefix}.contentSize"
            if size_path in link_map:
                cl = resp.headers.get("Content-Length", "")
                if cl and link_map[size_path]["value"] == cl:
                    auto_paths.add(size_path)

    # ── Field column verification — check name/extract.column against file headers ──
    # Build dist @id → {url, format} for all file types whose columns can be read
    _READABLE_EXTS = (".csv", ".tsv", ".gz", ".jsonl", ".ndjson", ".json", ".parquet")
    dist_content_map: dict[str, dict] = {}
    for l in links:
        m = _DIST_PFX_RE.match(l["path"])
        if m and l["value"]:
            pfx = m.group(1)
            url = link_map.get(f"{pfx}.contentUrl", {}).get("value", "")
            fmt = link_map.get(f"{pfx}.encodingFormat", {}).get("value", "")
            url_lc = url.lower().split("?")[0]
            is_readable = url.startswith("http") and (
                    url_lc.endswith(_READABLE_EXTS)
                    or any(t in fmt.lower() for t in ("csv", "tsv", "tab", "json", "parquet"))
            )
            if is_readable:
                dist_content_map[l["value"]] = {"url": url, "format": fmt}

    # Build field prefix → {column, name, dist_id}
    field_meta: dict[str, dict] = {}
    for l in links:
        for pat, key in ((_FIELD_COL_RE, "column"), (_FIELD_NM_RE, "name"), (_FIELD_SRCID_RE, "dist_id")):
            mc = pat.match(l["path"])
            if mc:
                pfx = mc.group(1)
                fd = field_meta.setdefault(pfx, {})
                if key == "dist_id":
                    fd.setdefault("dist_id", l["value"])
                else:
                    fd[key] = l["value"]
                break

    # Fetch headers concurrently for unique text file URLs
    seen_urls: set[str] = set()
    header_tasks: list[tuple[str, str]] = []
    for fd in field_meta.values():
        dist_id = fd.get("dist_id")
        if dist_id and dist_id in dist_content_map:
            url = dist_content_map[dist_id]["url"]
            if url not in seen_urls:
                seen_urls.add(url)
                header_tasks.append((url, dist_content_map[dist_id]["format"]))

    file_columns: dict[str, list[str] | None] = {}
    if header_tasks:
        with ThreadPoolExecutor(max_workers=10) as pool:
            futures_map = {pool.submit(_fetch_file_columns, url, fmt): url
                           for url, fmt in header_tasks}
            for future in as_completed(futures_map):
                file_columns[futures_map[future]] = future.result()

    # Approve fields whose values appear in the source file's header
    for pfx, fd in field_meta.items():
        dist_id = fd.get("dist_id")
        if not dist_id or dist_id not in dist_content_map:
            continue
        cols = file_columns.get(dist_content_map[dist_id]["url"])
        if not cols:
            continue
        if fd.get("column") in cols:
            auto_paths.add(f"{pfx}.source.extract.column")

    issues = flag_issues(links, url_results)
    return auto_paths, issues


# ── LLM feedback report ───────────────────────────────────────────────────────

def write_llm_report(all_issues: list[dict], target_kbs: list[str], path: str):
    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    by_type: dict[str, list[dict]] = defaultdict(list)
    for iss in all_issues:
        by_type[iss["issue_type"]].append(iss)

    lines = [
        "# Croissant Metadata Quality Feedback",
        "",
        f"**Generated**: {now}  ",
        f"**Knowledge bases reviewed**: {', '.join(target_kbs)}  ",
        f"**Total issues found**: {len(all_issues)}",
        "",
        "This report is intended to be shared with the LLM system that originally",
        "generated this Croissant metadata. Each section describes a class of error,",
        "explains why it violates the Croissant specification, provides examples from",
        "the actual data, and gives instructions for generating correct output in future.",
        "",
        "---",
        "",
    ]

    if not all_issues:
        lines.append("No issues were detected. The generated metadata passed all checks.")
    else:
        # Summary table
        lines += ["## Summary", ""]
        lines.append("| Issue type | Count | Severity |")
        lines.append("|---|---|---|")
        for itype, issues in sorted(by_type.items(), key=lambda x: -len(x[1])):
            meta = ISSUE_META.get(itype, {})
            label = meta.get("label", itype)
            sev = meta.get("severity", "info").upper()
            lines.append(f"| {label} | {len(issues)} | {sev} |")
        lines += ["", "---", ""]

        # Per-issue-type sections
        for itype, issues in sorted(by_type.items(), key=lambda x: -len(x[1])):
            meta = ISSUE_META.get(itype, {})
            label = meta.get("label", itype)
            sev = meta.get("severity", "info").upper()

            lines += [
                f"## {label}  `{itype}`",
                f"**Severity**: {sev} — **Count**: {len(issues)}",
                "",
            ]

            if meta.get("why"):
                lines += ["### Why this is wrong", "", meta["why"], ""]

            if meta.get("fix"):
                lines += ["### What to do instead", "", meta["fix"], ""]

            # Deduplicate examples (show up to 8 distinct values)
            seen_vals: set[str] = set()
            examples = []
            for iss in issues:
                key = f"{iss['kb_name']}|{iss['path']}|{iss['value']}"
                if key not in seen_vals:
                    seen_vals.add(key)
                    examples.append(iss)
                if len(examples) >= 8:
                    break

            lines += ["### Examples from your data", ""]
            for ex in examples:
                val_preview = ex["value"][:100] + ("…" if len(ex["value"]) > 100 else "")
                lines.append(f"- **{ex['kb_name']}** `{ex['path']}`")
                lines.append(f"  Value: `{val_preview}`")
                lines.append(f"  _{ex['detail']}_")
            if len(issues) > len(examples):
                lines.append(f"- … and {len(issues) - len(examples)} more occurrences")

            lines += ["", "---", ""]

    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


# ── Structural issue checks (operates on full metadata JSON) ─────────────────

_PLACEHOLDER_FIELD_RE = re.compile(r"^column_\d+$", re.IGNORECASE)
_COMP_EXTS = (".zip", ".tar.gz", ".tgz", ".gz")
_TABULAR_EXTS = ("tsv", "csv", "json", "jsonl", "ndjson", "parquet")


def _tabular_compressed_inner(url: str) -> str:
    """Return the inner tabular extension if url is a compressed tabular archive, else ''."""
    u = url.lower().split("?")[0]
    for ce in _COMP_EXTS:
        if u.endswith(ce):
            base = u[: -len(ce)]
            for te in _TABULAR_EXTS:
                if base.endswith(f".{te}") or base.endswith(f"_{te}") or base.endswith(f"-{te}"):
                    return te
            break
    return ""


_REQUIRED_TOP_LEVEL = ["name", "description", "url", "license", "conformsTo"]


def flag_structural_issues(metadata_json: str) -> list[dict]:
    """Detect spec violations that require reading the full metadata JSON."""
    try:
        metadata = json.loads(metadata_json, strict=False)
    except Exception:
        return []

    issues: list[dict] = []

    # ── 1. Missing required top-level fields ─────────────────────────────────
    for field in _REQUIRED_TOP_LEVEL:
        val = metadata.get(field)
        if not val:
            issues.append({
                "path": field,
                "value": "",
                "issue_type": "missing_required_field",
                "detail": f"Required top-level field '{field}' is absent.",
            })

    # ── 2. Compressed archives with tabular content and no inner FileObject ──
    dists = metadata.get("distribution", [])
    if isinstance(dists, list):
        dist_ids = {d.get("@id") for d in dists if isinstance(d, dict)}
        contained_parents: set[str] = set()
        for d in dists:
            if not isinstance(d, dict):
                continue
            ci = d.get("containedIn", {})
            if isinstance(ci, dict):
                contained_parents.add(ci.get("@id", ""))
            elif isinstance(ci, str):
                contained_parents.add(ci)

        for idx, d in enumerate(dists):
            if not isinstance(d, dict):
                continue
            did = d.get("@id", f"distribution[{idx}]")
            url = str(d.get("contentUrl", ""))
            inner_ext = _tabular_compressed_inner(url)
            if inner_ext and did not in contained_parents and not d.get("containedIn"):
                issues.append({
                    "path": f"distribution[{idx}].contentUrl",
                    "value": url.split("?")[0],
                    "issue_type": "zip_without_inner_file",
                    "detail": (
                        f"This archive appears to contain a .{inner_ext} file, "
                        f"but no cr:FileObject with containedIn pointing to '{did}' "
                        f"is defined. RecordSet fields cannot reference the inner file."
                    ),
                })

    # ── 3. RecordSet checks ───────────────────────────────────────────────────
    record_sets = metadata.get("recordSet", [])
    if not isinstance(record_sets, list):
        return issues

    for idx, rs in enumerate(record_sets):
        if not isinstance(rs, dict):
            continue
        fields = rs.get("field", [])
        rs_id = rs.get("@id") or rs.get("name") or f"recordSet[{idx}]"

        # 3a. Inline data rows should be examples when fields source from a distribution
        data_rows = rs.get("data", [])
        if isinstance(data_rows, list) and data_rows:
            rs_fields = rs.get("field", [])
            fields_have_source = any(
                isinstance(f, dict) and f.get("source", {}).get("fileObject") or
                isinstance(f, dict) and f.get("source", {}).get("fileSet")
                for f in rs_fields
            )
            if fields_have_source:
                issues.append({
                    "path": f"recordSet[{idx}].data",
                    "value": f"{len(data_rows)} rows",
                    "issue_type": "inline_data_rows",
                    "detail": (
                        f"recordSet '{rs_id}' has {len(data_rows)} inline data row(s) "
                        f"but its fields source from distribution files. "
                        f"Rename 'data' to 'examples' — these are sample rows, "
                        f"not the complete dataset."
                    ),
                })

        # 3b. Fields missing source — report one issue per recordSet (not per field)
        missing_src = [
            f for f in fields
            if isinstance(f, dict) and not f.get("source")
        ]
        if missing_src:
            issues.append({
                "path": f"recordSet[{idx}]",
                "value": rs_id,
                "issue_type": "missing_field_source",
                "detail": (
                    f"{len(missing_src)} of {len(fields)} fields in recordSet '{rs_id}' "
                    f"have no 'source' property. Without source.fileObject.@id and "
                    f"source.extract.column, these fields cannot be used to load data."
                ),
            })

        # 3c. Placeholder field names
        for fidx, f in enumerate(fields):
            if not isinstance(f, dict):
                continue
            fname = f.get("name", "")
            if fname and _PLACEHOLDER_FIELD_RE.match(str(fname)):
                issues.append({
                    "path": f"recordSet[{idx}].field[{fidx}].name",
                    "value": fname,
                    "issue_type": "placeholder_field_name",
                    "detail": (
                        f"Field name '{fname}' is a generic placeholder. "
                        f"The LLM could not read the actual column headers from this file. "
                        f"Replace with the real column name from the source file."
                    ),
                })

    # Note: recordSets with identical field schemas are NOT flagged.
    # Croissant 1.1 has no schema inheritance, so separate recordSets for
    # semantically distinct datasets that share column structure is correct.

    return issues


# ── CLI ───────────────────────────────────────────────────────────────────────

def ensure_issues_table(cur):
    cur.execute("""
                CREATE TABLE IF NOT EXISTS validation_issues
                (
                    id
                    INTEGER
                    PRIMARY
                    KEY
                    AUTOINCREMENT,
                    kb_name
                    TEXT
                    NOT
                    NULL,
                    issue_type
                    TEXT
                    NOT
                    NULL,
                    path
                    TEXT
                    NOT
                    NULL,
                    value
                    TEXT
                    NOT
                    NULL,
                    detail
                    TEXT
                    NOT
                    NULL,
                    created_at
                    TEXT
                    NOT
                    NULL
                )
                """)


def main():
    parser = argparse.ArgumentParser(description="Validate Croissant metadata.")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--db", metavar="PATH", help=f"SQLite database (default: {DB_PATH})")
    source.add_argument("--json", metavar="PATH", help="Croissant metadata JSON file")
    parser.add_argument("--links", metavar="PATH", help="persistent_fields.json companion for --json")
    parser.add_argument("--dry-run", action="store_true", help="preview database changes")
    parser.add_argument("--kb", action="append", default=[], help="validate only this database KB")
    parser.add_argument("--reset", action="store_true", help="clear prior database validation first")
    args = parser.parse_args()

    if args.json and not args.links:
        parser.error("--links is required with --json")
    if args.links and not args.json:
        parser.error("--links requires --json")
    if args.json and (args.kb or args.reset):
        parser.error("--kb and --reset apply only to database input")

    conn = None
    cur = None
    inputs: list[tuple[str, list[dict], str]] = []

    if args.json:
        try:
            with open(args.json, encoding="utf-8") as f:
                metadata_json = f.read()
            metadata = json.loads(metadata_json, strict=False)
            with open(args.links, encoding="utf-8") as f:
                links = json.load(f, strict=False)
        except (OSError, json.JSONDecodeError) as exc:
            parser.error(str(exc))
        if not isinstance(metadata, dict):
            parser.error("--json must contain a JSON object")
        if not isinstance(links, list) or any(
                not isinstance(link, dict) or not {"path", "value", "url"} <= link.keys()
                for link in links):
            parser.error("--links must contain a JSON array of objects with path, value, and url")
        inputs.append((str(metadata.get("name") or os.path.basename(args.json)), links, metadata_json))
    else:
        conn = sqlite3.connect(args.db or DB_PATH)
        conn.row_factory = sqlite3.Row
        cur = conn.cursor()
        ensure_issues_table(cur)
        cur.execute("SELECT name FROM knowledge_bases ORDER BY name")
        all_kbs = [r["name"] for r in cur.fetchall()]
        target_kbs = [kb for kb in all_kbs if not args.kb or kb in args.kb]
        for kb_name in target_kbs:
            cur.execute("SELECT path, value, url FROM kb_links WHERE kb_name = ?", (kb_name,))
            links = [dict(r) for r in cur.fetchall()]
            cur.execute("SELECT croissant_metadata FROM knowledge_bases WHERE name = ?", (kb_name,))
            meta_row = cur.fetchone()
            inputs.append((kb_name, links, (meta_row["croissant_metadata"] if meta_row else None) or "{}"))

    target_kbs = [kb_name for kb_name, _, _ in inputs]

    if not target_kbs:
        print("No matching knowledge bases found.")
        if conn:
            conn.close()
        return

    if args.reset and not args.dry_run:
        kb_ph = ",".join("?" * len(target_kbs))
        cur.execute(
            f"UPDATE kb_links SET auto_reviewed = 0 WHERE kb_name IN ({kb_ph})",
            target_kbs,
        )
        cur.execute(
            f"DELETE FROM validation_issues WHERE kb_name IN ({kb_ph})",
            target_kbs,
        )
        print(f"Reset auto_reviewed and cleared old issues for {len(target_kbs)} KB(s).\n")

    now_str = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    grand_auto = 0
    all_issues: list[dict] = []

    for kb_name, links, metadata_json in inputs:
        print(f"{kb_name}: {len(links)} fields — validating…", flush=True)

        passing, issues = validate_kb(links)
        issues = issues + flag_structural_issues(metadata_json)

        issue_counts = Counter(iss["issue_type"] for iss in issues)
        print(f"  ✓ {len(passing)} auto-verified", end="")
        if issue_counts:
            print(f"  |  ⚑ {sum(issue_counts.values())} issues flagged "
                  f"({', '.join(f'{v} {k}' for k, v in issue_counts.most_common())})", end="")
        print()

        for iss in issues:
            iss["kb_name"] = kb_name
        all_issues.extend(issues)
        grand_auto += len(passing)

        if conn and not args.dry_run:
            # Write auto_reviewed
            passing_list = sorted(passing)
            for start in range(0, len(passing_list), 500):
                chunk = passing_list[start:start + 500]
                ph = ",".join("?" * len(chunk))
                cur.execute(
                    f"UPDATE kb_links SET auto_reviewed = 1 "
                    f"WHERE kb_name = ? AND path IN ({ph})",
                    [kb_name] + chunk,
                )
            # Replace issues for this KB
            cur.execute("DELETE FROM validation_issues WHERE kb_name = ?", (kb_name,))
            cur.executemany(
                "INSERT INTO validation_issues (kb_name, issue_type, path, value, detail, created_at) "
                "VALUES (?, ?, ?, ?, ?, ?)",
                [(kb_name, iss["issue_type"], iss["path"], iss["value"], iss["detail"], now_str)
                 for iss in issues],
            )

    # ── LLM feedback report ───────────────────────────────────────────────────
    if all_issues or not args.dry_run:
        os.makedirs(REPORTS_DIR, exist_ok=True)
        ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        report_path = os.path.join(REPORTS_DIR, f"llm_feedback_{ts}.md")
        write_llm_report(all_issues, target_kbs, report_path)
        print(f"\n{'[dry-run] ' if args.dry_run else ''}Report written to {report_path}")

    if args.json:
        print(f"Done.  {grand_auto} fields auto-verified | {len(all_issues)} issues flagged.")
    elif args.dry_run:
        print("Dry run — no DB changes written.  Re-run without --dry-run to apply.")
    else:
        conn.commit()
        print(f"Done.  {grand_auto} fields auto-verified | "
              f"{len(all_issues)} issues written to dashboard.")

    if conn:
        conn.close()


if __name__ == "__main__":
    main()
