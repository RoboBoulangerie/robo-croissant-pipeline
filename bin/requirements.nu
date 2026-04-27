#!/usr/bin/env nu

def main [] {

    let mlcroissant_binary_exists = "../.venv/bin/mlcroissant" | path exists

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
