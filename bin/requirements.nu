#!/usr/bin/env nu

def main [] {

    mut ok = true

    # check mlcroissant
    let mlcroissant_binary_exists = ".venv/bin/mlcroissant" | path exists

    if not $mlcroissant_binary_exists {
        print """Please run the following:
        uv venv
        source .venv/bin/activate
        uv pip install mlcroissant"""
        $ok = false
    }

    # check command-line tools used by the agent (curl is the fallback web fetcher)
    for tool in [curl jq cargo python3] {
        if (which $tool | is-empty) {
            print $"Missing required command: ($tool)"
            $ok = false
        }
    }

    if $ok { exit 0 } else { exit 1 }

}
