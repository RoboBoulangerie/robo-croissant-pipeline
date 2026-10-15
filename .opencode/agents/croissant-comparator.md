---
description: Compares two Croissant JSON-LD metadata files from environment-variable paths and writes comparison.md.
mode: all
---

Compare the two Croissant JSON-LD files identified by `RCP_OUTPUT_PATH_1` and
`RCP_OUTPUT_PATH_2`, then write the complete result to `comparison.md` in the
current working directory.

Follow these requirements:

1. Read both paths from the environment. Stop with a clear error if either
   variable is unset, either path is not a readable regular file, or either
   file is invalid JSON. Do not create `comparison.md` after a validation
   failure.
2. Compare parsed JSON values, not raw text. Ignore object-key ordering and
   formatting differences. Treat array order as meaningful only when the
   Croissant structure makes it meaningful; otherwise match entries by `@id`,
   `name`, `contentUrl`, or another stable identity before comparing them.
3. Cover dataset metadata and Croissant structures, including `distribution`,
   `FileObject`, `FileSet`, `RecordSet`, fields, data types, source mappings,
   references, keys, and repeated/nested fields when present.
4. Identify additions, removals, changed values, type changes, and structural
   changes. Include JSON Pointer-style paths for specific differences.
5. Distinguish substantive metadata/schema changes from ordering or formatting
   differences. Do not paste either complete JSON document into the report.
6. Write valid Markdown with these sections: title, input files, executive
   summary, comparison metrics, metadata differences, distribution differences,
   RecordSet and field differences, other structural differences, and conclusion.
   Use compact tables or lists and explicitly say `None` in sections with no
   differences.
7. Report the number of additions, removals, modifications, and type changes.
   If the parsed documents are equivalent, state that prominently.
8. Overwrite an existing `comparison.md`. Do not modify either input file or
   create any other output.

After writing the report, verify that `comparison.md` exists and is non-empty,
then return its path and a one-sentence summary.
