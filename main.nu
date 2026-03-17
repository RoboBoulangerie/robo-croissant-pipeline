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
            #print $f
            let cleaned_html = open $f | tidy -wrap 3000 -indent -q --custom-tags blocklevel | htmlq -wp --remove-nodes "script,style,link"
            echo $cleaned_html | save -f $f
        }
    } catch {|e| print $e }
}

def crawl_croissant_spec [tmp_dir: string] {
    try {
        let source_url = "https://docs.mlcommons.org/croissant/docs/croissant-spec.html"
        spider --url $source_url -d 3 download -t $tmp_dir
    } catch {|e| print $e }
}

def clean_ai_json_text [raw: string] {
    $raw
    | str replace -a '```json' ''
    | str replace -a '```' ''
    | str trim
}

def parse_json_with_repair [raw: string] {
    let cleaned = (clean_ai_json_text $raw)
    try {
        $cleaned | from json
    } catch {
        let repair_prompt = $"
Convert the following text into strict valid RFC 8259 JSON.
Return only JSON with no markdown code fences.
Replace placeholders like `...` with valid JSON values.

($cleaned)
"
        let repaired_raw = (aichat $"($repair_prompt)")
        let repaired_text = (clean_ai_json_text $repaired_raw)
        $repaired_text | from json
    }
}

def main [] {
    stor reset
    stor create --table-name "knowledge_sources" --columns { name: str, url: str, croissant_metadata: jsonb}
    stor create --table-name "knowledge_source_mappings" --columns { source_name: str, key: str, answer: str, url: str }

    let home_dir = $nu.home-dir
#    if not ($"($home_dir)/.config/aichat/config.yaml" | path exists) { (write_aichat_config $home_dir) }

    let croissant_spec_tmp_dir = mktemp -d -p .
    (crawl_croissant_spec $croissant_spec_tmp_dir)

    rm --force "robo_croissant.db"

    let config = open config.toml

    mut enabled_sources = ($config | get knowledge_sources)

    for source in $enabled_sources {
        print $source.name
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

        #break

        let persistent_fields_prompt = $config | get persistent_fields_prompt
        let persistent_fields_response = aichat -f $tmp_dir $"($persistent_fields_prompt)" | str replace '```json' '' | str replace '```' '' | from json
        for pfr in $persistent_fields_response {
            try {
                stor insert --table-name "knowledge_source_mappings" --data-record { source_name: $source_name, key: $pfr.key, answer: $pfr.value, url: $pfr.url }
            } catch {|e| print $e }
        }

        let croissant_metadata_prompt = $config | get croissant_metadata_prompt | str replace '%name%' $"($source.name)"
        let cr_answer = aichat -f $tmp_dir -f $croissant_spec_tmp_dir $"($croissant_metadata_prompt)" | str replace '```json' '' | str replace '```' ''
        
        #$cr_answer | print
        mut cr_answer_json = (parse_json_with_repair $cr_answer)

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
