#!/usr/bin/env nu

def main [--input: path] {
    let names = [
        'DB' 'DB Object ID' 'DB Object Symbol' 'Relation' 'GO ID'
        'Reference' 'Evidence Code' 'With (or) From' 'Aspect'
        'DB Object Name' 'DB Object Synonym' 'DB Object Type' 'Taxon'
        'Date' 'Assigned By' 'Annotation Extension' 'Gene Product Form ID'
    ]
    let required = [0 1 2 3 4 5 6 8 11 12 13 14]

    # shortcut: validate structure, not ontology IDs or evidence-code rules; use a GO validator for semantic checks.
    # shortcut: inspect first 100 lines only (no full gzip integrity check); scan the whole file if complete validation is needed.
    let state = (^gzip -cdf -- $input | lines | first 100 | enumerate | reduce -f {headers: {}, rows: 0} {|entry, state|
        let line = $entry.item
        let number = $entry.index + 1
        if ($line | str starts-with '!') {
            if $state.rows > 0 {
                error make {msg: $"GAF header after data at line ($number)"}
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
            if ($values | length) != 17 {
                error make {msg: $"GAF line ($number): expected 17 tab-delimited fields, found ($values | length)"}
            }
            for index in $required {
                if ($values | get $index | is-empty) {
                    error make {msg: $"GAF line ($number): missing required field ($names | get $index)"}
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
