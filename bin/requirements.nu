#!/usr/bin/env nu

def main [] {

    # check for RCP_KB_NAME environment variable
    if not ("RCP_KB_NAME" in $env) {
        echo """Please prefix the command with 'RCP_KB_NAME=<name from knowledge_bases.toml file>'"""
        exit 1
    }

    # check mlcroissant
    let mlcroissant_binary_exists = ".venv/bin/mlcroissant" | path exists

    if $mlcroissant_binary_exists {
        exit 0
    } else {
        echo """Please run the following:
        uv venv
        source .venv/bin/activate
        uv pip install mlcroissant"""
        exit 1
    }

}

