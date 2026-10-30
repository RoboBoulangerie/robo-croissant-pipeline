#!/usr/bin/env nu

use std/dirs

def main [outputs_path_1: path, outputs_path_2: path, --harness: string, --model: string] {
    let harness = $harness | default ($env.RCP_HARNESS? | default "opencode")
    let model = $model | default ($env.RCP_MODEL? | default "")
    let run_agent = $env.FILE_PWD | path join "run_agent.nu"
    for kb in (open knowledge-bases.toml | get knowledge_bases | where $it.enabled == true) {
        print $"Starting: ($kb.name)"
        let comparison_report_dir = $"./comparison_reports/($kb.name)"
        mkdir $comparison_report_dir
        dirs add $comparison_report_dir
        # Should be like ./outputs/ 
        RCP_OUTPUT_PATH_1=$"($outputs_path_1)/($kb.name)/1/croissant.json" RCP_OUTPUT_PATH_2=$"($outputs_path_2)/($kb.name)/1/croissant.json" RCP_MODEL=$model nu $run_agent croissant-comparator "run" --harness $harness
        dirs drop
        print $"Finished: ($kb.name)"
    }
}
