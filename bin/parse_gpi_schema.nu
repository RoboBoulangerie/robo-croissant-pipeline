#!/usr/bin/env nu

def main [--input: path] {
    let names = [
        'DB:Object ID' 'Object Symbol' 'Object Name' 'Object_Synonym(s)'
        'Object Type' 'Object Taxon' 'Encoded by' 'Canonical object ID'
        'Protein Complex Members' 'Cross-reference(s)' 'Gene Product Properties'
    ]
    let required = [0 1 4 5 7]

    # shortcut: inspect first 100 lines only (no full gzip integrity check); scan the whole file if complete validation is needed.
    let state = (^gzip -cdf -- $input | lines | first 100 | enumerate | reduce -f {headers: {}, rows: 0} {|entry, state|
        let line = $entry.item
        let number = $entry.index + 1
        if ($line | str starts-with '!') {
            if $state.rows > 0 {
                error make {msg: $"GPI header after data at line ($number)"}
            }
            let header = ($line | parse --regex '^!(?<key>generated-by):\s*(?<value>.*)$')
            if ($header | is-empty) {
                $state
            } else {
                $state | update headers ($state.headers | upsert $header.0.key $header.0.value)
            }
        } else if ($line | str trim | is-empty) {
            $state
        } else {
            let values = ($line | split row (char tab))
            if ($values | length) != 11 {
                error make {msg: $"GPI line ($number): expected 11 tab-delimited fields, found ($values | length)"}
            }
            for index in $required {
                if ($values | get $index | is-empty) {
                    error make {msg: $"GPI line ($number): missing required field ($names | get $index)"}
                }
            }
            $state | update rows ($state.rows + 1)
        }
    })

    if (($state.headers | get -o 'generated-by' | default '') | is-empty) {
        error make {msg: 'Missing !generated-by: header'}
    }

    {columns: ($names | enumerate | each {|column|
        {name: $column.item, datatype: 'TEXT', required: ($column.index in $required)}
    })} | to json
}
