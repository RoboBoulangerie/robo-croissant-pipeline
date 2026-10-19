#!/usr/bin/env nu

use std/dirs

def main [--kb_file: path = "knowledge-bases.toml"] {
    mkdir outputs
    for name in (open $kb_file | get knowledge_bases | where $it.enabled == true | get name) {
        print $"Starting: ($name)"
        RCP_KB_INPUT=$kb_file RCP_KB_NAME=$"($name)" opencode run --agent croissant-metadata-generator "run"
        print $"Finished: ($name)"
    }
}
