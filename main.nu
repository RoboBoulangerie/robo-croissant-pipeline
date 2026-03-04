#!/usr/bin/env nu

def crawl_knowledge_source [
    source_url: string,
    source_name: string,
    depth: int,
    blacklist: string,
    tmp_dir: string
] {
    try {
        mut blacklist_arg = ""
        if ($blacklist | is-not-empty) {
            spider --url $source_url -d $depth --blacklist-url $"($blacklist)" download -t $tmp_dir
        } else {
            spider --url $source_url -d $depth download -t $tmp_dir
        }
        let files = ls ...(glob ($tmp_dir)/**/*) | where type == file | get name
        for f in $files {
            open $f | tidy -wrap 3000 -quiet | str replace -a -r '<style[^>]*>[\s\S]*?<\/style>' '' | str replace -a -r '<script[^>]*>[\s\S]*?<\/script>' '' | str replace -a -r '<g[^>]*>[\s\S]*?<\/g>' '' | save -f $f
        }
    } catch {|e| echo $e }
}

def crawl_croissant_spec [tmp_dir: string] {
    try {
        let source_url = "https://docs.mlcommons.org/croissant/docs/croissant-spec.html"
        spider --url $source_url -d 3 download -t $tmp_dir
    } catch {|e| echo $e }
}

def write_aichat_config [home_dir: string] {
    if not ($env.RC_MODEL) {
        echo "RC_MODEL environment variable must be set"
        exit 1
    }
    if not ($env.RC_CLIENTS_TYPE) {
        echo "RC_CLIENTS_TYPE environment variable must be set"
        exit 1
    }
    if not ($env.RC_CLIENTS_API_KEY) {
        echo "RC_CLIENTS_API_KEY environment variable must be set"
        exit 1
    }
    try {
        let config = {
            "model": $env.MODEL
            "clients": [
                {
                    "type": $env.CLIENTS_TYPE
                    "api_key": $env.CLIENTS_API_KEY
                }
            ]
        }
        mkdir $"($home_dir)/.config/aichat"
        $config | to yaml | save -f /home/rc/.config/aichat/config.yaml
    } catch {|e| echo $e }
}

def main [] {
    stor reset
    stor create --table-name "knowledge_sources" --columns { name: str, url: str, croissant_metadata: jsonb}
    stor create --table-name "knowledge_source_mappings" --columns { name: str, key: str, answer: str, url: str }

    let home_dir = $nu.home-path
    if not ($"($home_dir)/.config/aichat/config.yaml" | path exists) { (write_aichat_config home_dir) }

    let croissant_spec_tmp_dir = mktemp -d -p .
    (crawl_croissant_spec $croissant_spec_tmp_dir)

    rm --force "robo_croissant.db"

    let config = open config.toml

    let prompts = $config | get prompts
    mut enabled_sources = ($config | get knowledge_sources | where {|source| $source.enabled == true})

    if ("RC_TARGETED_KNOWLEDGE_SOURCES" in $env) {
        let targeted_knowledge_sources = $env.RC_TARGETED_KNOWLEDGE_SOURCES | split row ","
        if (($targeted_knowledge_sources | length) > 0) { $enabled_sources = ($enabled_sources | where {|source| $source.name in $targeted_knowledge_sources }) }
    }

    for source in $enabled_sources {
        let tmp_dir = mktemp -d -p .

        let source_name = $source.name
        let source_url = $source.url

        mut source_blacklist = ""
        if "blacklist" in $source {
            $source_blacklist = $source.blacklist
        }
        #$source_blacklist | print

        mut source_depth = 2
        if "depth" in $source {
            $source_depth = $source.depth
        }

        (crawl_knowledge_source $source_url $source_name $source_depth $source_blacklist $tmp_dir)

        mut results_table = (
            $prompts | get key | each {|k| { $k: null } } | reduce {|it| merge $it}
        )

        for p in $prompts {
            try {
                let answer = aichat -f $tmp_dir $"($p.prompt)"
                let key_answer =  $answer | query json $p.key
                #$key_answer | print
                let url_answer =  $answer | query json url
                stor insert --table-name "knowledge_source_mappings" --data-record { name: $source_name, key: $p.key, answer: $key_answer, url: $url_answer }
            } catch {|e| echo $e }
        }

        let croissant_metadata_prompt = $config | get croissant_metadata | str replace '%name%' $"($source.name)"
        let cr_answer = aichat -f $tmp_dir -f $croissant_spec_tmp_dir $"($croissant_metadata_prompt)" | str replace '```json' '' | str replace '```' ''
        #$cr_answer | print
        mut cr_answer_json = $cr_answer | from json

        let mapping_data = stor open | query db "select key, answer from knowledge_source_mappings" | reduce -f {} {|it, acc| $acc | upsert $it.key $it.answer }
        #$mapping_data | print

        $cr_answer_json = ($cr_answer_json | merge $mapping_data)
        #$cr_answer_json | print

        stor insert --table-name "knowledge_sources" --data-record { name: $source_name, url: $source_url, croissant_metadata: $cr_answer_json }

        #rm --recursive $tmp_dir
    }
    #rm --recursive croissant_spec_tmp_dir
    stor export --file-name "robo_croissant.db" | ignore
}
