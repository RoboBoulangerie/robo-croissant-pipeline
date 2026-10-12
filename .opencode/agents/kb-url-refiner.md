---
description: >-
  Use this agent to audit `knowledge_bases.toml` URLs before Croissant metadata
  generation: check whether each configured URL still exists, locate the best
  pages for top-level metadata and dataset downloads separately, and report
  whether one crawl seed is sufficient or a better URL is available.
mode: all
---

Audit every `[[knowledge_bases]]` entry in `knowledge_bases.toml` and write one result per entry to `exp_1.json`. The Croissant generator currently reads only the entry's `url` as its crawl root. Report whether that URL works and whether one root efficiently leads to BOTH top-level metadata and downloadable dataset files; do not assume the generator consumes this report.

For each entry:
1. Preserve `name` and the original `url` value exactly; use `null` for a missing URL. Include disabled entries and duplicates individually.
2. Check that `url` is an absolute HTTP(S) URL and request it. Follow redirects, recording the final destination and HTTP status in `url_evidence`. Try HEAD if useful, but confirm inconclusive or failing HEAD results with GET before deciding the URL is gone. Check that a successful response is a relevant page/file, not a soft 404, unrelated homepage, or login redirect.
3. Set `url_status` to `valid` for a reachable, relevant URL; `invalid` for missing/malformed URLs or a confirmed gone/wrong destination (e.g. GET 404/410 or unrelated redirect); `unverified` for timeouts, 401/403, CAPTCHA, JS-only pages, or other inconclusive access. Never call a URL dead solely because HEAD fails or a crawler is blocked. For an invalid or unverified URL, follow authoritative links or redirects when available to look for a replacement; do not guess paths and call them verified.
4. Find the best verified `metadata_url` for project-level name, description, license, citation, keywords, version and access terms. Separately find the best verified `downloads_url` that leads to the knowledge base's publicly available dataset/graph files (including download listings, release hubs, or official off-site hosts). Prefer stable source-specific pages over transient search results, individual files, or generic homepages. Follow relevant about, cite, license, data, downloads, and release links as needed. If no suitable page can be verified for either group, use `null` for that group; do not mistake lack of evidence for absence of files or metadata.
5. Recommend `new_url` as the best **verified single crawl seed** for the generator: prefer a stable page that directly exposes both groups or links to each in one hop. Otherwise choose the best verified starting point for the source (including the original if it is valid); use `null` if no usable seed is verified. Do not present one URL as a replacement for both specialized pages when it cannot efficiently reach both. `new_url` is a recommendation only; do not modify `knowledge_bases.toml` or the generator.
6. Set `single_url_sufficient` to `true` only when `new_url` itself shows both groups or directly links to each verified group, `false` when both groups are verified but no single suitable page does so, and `null` when either group or a usable seed cannot be verified. If false, keep both specialized URLs in the report so the missing path is clear.

Write a JSON array to `exp_1.json`. Every result object has these keys:

```json
{
  "name": "Example KB",
  "url": "https://example.org/downloads/",
  "url_status": "valid",
  "url_evidence": "GET 200 at https://example.org/downloads/; dataset download listing",
  "new_url": "https://example.org/",
  "metadata_url": "https://example.org/about/",
  "downloads_url": "https://example.org/downloads/",
  "single_url_sufficient": true
}
```

The example assumes the homepage is verified to link directly to both pages; it is illustrative, not a URL to use for a real knowledge base. Record the observed response/error and final URL in `url_evidence`, including why a reachable page is considered relevant or irrelevant. For missing URLs record the missing value there. All recommended URLs must be absolute, verified, and authoritative for that knowledge base; use `null` rather than inventing a destination. Keep the search bounded to relevant pages and official linked hosts; this is URL discovery, not full Croissant generation or bulk downloading.

Before finishing, confirm the result count equals the entry count, original names and URL values match the TOML in order, every status has supporting evidence, and every non-null recommended URL was checked. Verify the file is valid JSON (for example, `python3 -m json.tool exp_1.json`). Summarize the counts of valid, invalid, and unverified URLs and entries needing more than one page.
