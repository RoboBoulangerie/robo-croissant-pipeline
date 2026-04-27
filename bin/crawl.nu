#!/usr/bin/env nu

def main [
    source_name: string,
    tmp_dir: string
] {

    let config = open config.toml

    mut enabled_sources = ($config | get knowledge_sources | where $it.name == $"($source_name)")

    let source_url = $enabled_sources | get url | get 0
    mut depth = 2
    if ($enabled_sources | columns | any { |c| $c == "depth" }) {
        $depth = $enabled_sources | get depth | get 0
    }
    mut blacklist = ""
    if ($enabled_sources | columns | any { |c| $c == "blacklist" }) {
        $blacklist = $enabled_sources | get blacklist | get 0
    }

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
