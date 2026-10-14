#!/usr/bin/env nu

use std/dirs

def main [] {
    mkdir outputs
    for name in (open knowledge_bases.toml | get knowledge_bases | where $it.enabled == true | get name) {
        print $"Starting: ($name)"
        RCP_KB_NAME=$"($name)" opencode run --agent croissant-metadata-generator "run"
        print $"Finished: ($name)"
    }
}