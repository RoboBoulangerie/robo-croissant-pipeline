#!/usr/bin/env nu

def main [] {
    rm --force "robo_croissant.db"
    stor create --table-name "knowledge_sources" --columns { name: str, url: str, croissant_metadata: jsonb }
    stor create --table-name "knowledge_source_mappings" --columns { source_name: str, key: str, answer: str, url: str }
    stor export --file-name "robo_croissant.db" | ignore
}
