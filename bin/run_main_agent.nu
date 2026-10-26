#!/usr/bin/env nu

use std/dirs

def main [--kb_file: path = "knowledge-bases.toml", --harness: string, --model: string] {
    let harness = $harness | default ($env.RCP_HARNESS? | default "opencode")
    let model = $model | default ($env.RCP_MODEL? | default "")
    let run_agent = $env.FILE_PWD | path join "run_agent.nu"
    mkdir outputs
    for name in (open $kb_file | get knowledge_bases | where $it.enabled == true | get name) {
        print $"Starting: ($name)"
        RCP_KB_INPUT=$kb_file RCP_KB_NAME=$"($name)" RCP_MODEL=$model nu $run_agent croissant-metadata-generator "run" --harness $harness
        print $"Finished: ($name)"
    }
}
