#!/usr/bin/env nu

def main [] {

    rm --force "robo_croissant.db"

    stor open | query db "CREATE TABLE IF NOT EXISTS knowledge_bases (name TEXT PRIMARY KEY, url TEXT, croissant_metadata JSONB)"
    stor open | query db "CREATE TABLE IF NOT EXISTS kb_links (kb_name TEXT, path TEXT, value TEXT, url TEXT, confidence REAL, reviewed BOOLEAN, auto_reviewed BOOLEAN, PRIMARY KEY (kb_name, path))"
    stor open | query db "CREATE TABLE IF NOT EXISTS runs (id INTEGER PRIMARY KEY AUTOINCREMENT, label TEXT NOT NULL, run_date TEXT, model TEXT, file_hash TEXT UNIQUE NOT NULL, imported_at TEXT NOT NULL)"
    stor open | query db "CREATE TABLE IF NOT EXISTS run_links (run_id INTEGER NOT NULL, kb_name TEXT NOT NULL, path TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY (run_id, kb_name, path))"
    stor open | query db "CREATE INDEX IF NOT EXISTS idx_kb_links_kb_path ON run_links(kb_name, path)"
    stor open | query db "CREATE INDEX IF NOT EXISTS idx_run_links_kb_path ON run_links(kb_name, path)"
    stor open | query db "CREATE TABLE IF NOT EXISTS validation_issues (id INTEGER PRIMARY KEY AUTOINCREMENT, kb_name TEXT NOT NULL, issue_type TEXT NOT NULL, path TEXT NOT NULL, value TEXT NOT NULL, detail TEXT NOT NULL, created_at TEXT NOT NULL)"

    for kb in (open knowledge_bases.toml | get knowledge_bases | where $it.enabled == true) {
        print $kb.name
        try {
            let first = ls ...(glob ./outputs/($kb.name)/**/croissant.json) | sort-by -r modified | first | get name | wrap first
            let second = ls ...(glob ./outputs/($kb.name)/**/persistent_fields.json) | sort-by -r modified | first | get name | wrap second
            let files = $first | merge $second
            for row in $files {
#                 print $row.first
#                 print $row.second
                let croissant_metadata_json = (open $row.first | to json)
                stor insert --table-name "knowledge_bases" --data-record { name: $"($kb.name)", url: $kb.url, croissant_metadata: $croissant_metadata_json }
                for pfr in (open $row.second) {
                    try {
                        stor insert --table-name "kb_links" --data-record { kb_name: $"($kb.name)", path: $pfr.path, value: $pfr.value, url: $pfr.url, confidence: $pfr.confidence, reviewed: false, auto_reviewed: false }
                    } catch {|e| print $e }
                }
            }
        } catch {|e| print $e }
#        break
    }

    stor export --file-name "robo_croissant.db" | ignore
}
