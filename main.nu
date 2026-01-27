#!/usr/bin/env nu

def crawl_knowledge_source [
    source_url: string,
    source_name: string
] {
    try {
        let blocked_suffixes = [".xsd", ".gz", ".obo", ".ppt", ".tsv"]
        let blocked_substrings = ["jsessionid", "?"]
        spider --url $source_url -d 3 crawl -o | lines | parse "{url}" | where {|row|
                let has_blocked_substring = ($blocked_substrings | any {|s| $row.url | str contains $s })
                let has_blocked_suffix = ($blocked_suffixes | any {|ext| $row.url | str ends-with $ext })
                (not $has_blocked_substring) and (not $has_blocked_suffix) and ($row.url != $source_url)} | get url | to text
    } catch {|e| echo $e }
}

def select_relevant_urls [
    source_url: string
    source_name: string
    formal_key: string
    tmp_dir: string
] {
    try {
        let urls = (crawl_knowledge_source $source_url $source_name)
        # need to persist to fs for aichat
        let dest_file = $"($tmp_dir)/($source_name)_urls.txt"
        $urls | save -f $dest_file
        let query = $"What are the most likely URLs in this list to find information regarding a formal \"($formal_key)\" for ($source_name)?  Output the results only using space separated values and limit the results to 2."
        aichat -f $dest_file $query
    } catch {|e| echo $e }
}

def url_flags_from_space_separated [urls_line: string] {
    $urls_line | split row " " | where {|u| ($u | str length) > 0 } | each {|u| $" -f ($u)" } | str join
}

def render_prompt [prompt: string, source_name: string] {
    $prompt | str replace "%name%" $source_name
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
    stor create --table-name "knowledge_sources" --columns { name: str, croissant_metadata: json }
    let home_dir = $nu.home-path
    if not ($"($home_dir)/.config/aichat/config.yaml" | path exists) { (write_aichat_config home_dir) }
    let tmp_dir = mktemp -d
    mut enabled_sources = (open knowledge_sources.toml | get knowledge_sources | where {|source| $source.enabled == true})
    if ("RC_TARGETED_KNOWLEDGE_SOURCES" in $env) {
        let targeted_knowledge_sources = $env.RC_TARGETED_KNOWLEDGE_SOURCES | split row ","
        if (($targeted_knowledge_sources | length) > 0) { $enabled_sources = ($enabled_sources | where {|source| $source.name in $targeted_knowledge_sources }) }
    }
    for source in $enabled_sources {
        let source_name = $source.name
        let source_url = $source.url
        mut results_table = (
            $source.prompts | get key | each {|k| { $k: null } } | reduce {|it| merge $it}
        )
        $results_table = ($results_table | upsert "url" $"($source.url)")
        for prompt_spec in $source.prompts {
            let formal_key = $prompt_spec.key
            let selected_urls_line = (select_relevant_urls $source_url $source_name $formal_key $tmp_dir)
            let url_flags = (url_flags_from_space_separated $selected_urls_line)
            let prompt_text = (render_prompt $prompt_spec.prompt $source_name)
            try {
                let answer = aichat $"($url_flags) \"($prompt_text)\""
                $results_table = ($results_table | update $"($formal_key)" $"($answer)")
            } catch {|e| echo $e }
        }
        mut croissant_template = open "croissant_minimal.json"
        $croissant_template = ($croissant_template | merge $results_table)
        let croissant_metadata = ($croissant_template | to json)
        stor insert --table-name "knowledge_sources" --data-record { name: $source_name, croissant_metadata: $croissant_metadata }
    }
    rm --force "rc.db"
    stor export --file-name "rc.db"
}
