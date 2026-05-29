#!/usr/bin/env nu

def main [source_name: string, cr_metadata_json: path, cr_mappings: path] {
    stor import -f "robo_croissant.db" | ignore
    rm --force "robo_croissant.db"
    let source_url = open knowledge_bases.toml | get knowledge_bases | where $it.name == $"($source_name)" | get url | get 0
    let croissant_metadata_json = (open $cr_metadata_json | to json)
    stor insert --table-name "knowledge_bases" --data-record { name: $source_name, url: $source_url, croissant_metadata: $croissant_metadata_json }
    for pfr in (open $cr_mappings) {
        try {
            stor insert --table-name "kb_links" --data-record { kb_name: $source_name, path: $pfr.path, value: $pfr.value, url: $pfr.url, confidence: $pfr.confidence }
        } catch {|e| print $e }
    }
    stor export --file-name "robo_croissant.db" | ignore
}
