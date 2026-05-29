#!/usr/bin/env nu

def main [] {
    rm --force "robo_croissant.db"
    stor create --table-name "knowledge_bases" --columns { name: str, url: str, croissant_metadata: jsonb }
    stor create --table-name "kb_links" --columns { kb_name: str, path: str, value: str, url: str, confidence: float }
    stor export --file-name "robo_croissant.db" | ignore
}


