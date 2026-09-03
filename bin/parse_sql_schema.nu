#!/usr/bin/env nu

def main [sql_file: path] {
    let sql = open --raw $sql_file
    # ponytail: regex is not dialect-aware; use a SQL parser for SQL inside comments or literals.
    let statements = (($sql + ";")
        | parse --regex '(?is)(?<statement>\bCREATE\s+(?:OR\s+REPLACE\s+)?(?:TEMP(?:ORARY)?\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?.*?;)'
        | get statement
        | each {|statement| $statement | str trim })

    for statement in $statements {
        stor open | query db $statement | ignore
    }

    let database = stor open
    let tables = $database
        | query db "SELECT name FROM sqlite_schema WHERE type = 'table' AND substr(name, 1, 2) != '__' AND lower(name) NOT LIKE '%sequence%' ORDER BY name"
        | each {|table|
            {
                name: $table.name
                fields: ($database | query db --params [$table.name] "SELECT name, type AS datatype FROM pragma_table_info(?) ORDER BY cid")
            }
        }

    {tables: $tables} | to json
}
