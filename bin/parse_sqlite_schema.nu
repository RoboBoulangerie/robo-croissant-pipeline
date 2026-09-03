#!/usr/bin/env nu

def main [database: path] {
    let database = open $database
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
