#!/usr/bin/env nu

def main [source_name: string, cr_metadata_json: path, cr_mappings: path] {
    stor import -f "robo_croissant.db"
    rm --force "robo_croissant.db"
    let source_url = open config.toml | get knowledge_sources | where $it.name == $"($source_name)" | get url | get 0
    for pfr in (open $cr_mappings) {
        try {
            stor insert --table-name "knowledge_source_mappings" --data-record { source_name: $source_name, key: $pfr.key, answer: $pfr.value, url: $pfr.url }
        } catch {|e| print $e }
    }
    let mapping_data = stor open | query db "select key, answer from knowledge_source_mappings" | reduce -f {} {|it, acc| $acc | upsert $it.key $it.answer }
    let cr_answer_json = (open $cr_metadata_json | merge $mapping_data)
    stor insert --table-name "knowledge_sources" --data-record { name: $source_name, url: $source_url, croissant_metadata: $cr_answer_json }
    stor export --file-name "robo_croissant.db" | ignore
}
