# Welcome to RoboCroissant Pipeline

## Getting started

### Install Dependencies/Libraries

#### Install Rust

From https://rust-lang.org/tools/install/
```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### Install Nushell

Nushell is a modern, cross-platform shell and programming language written in Rust that treats all data as structured tables rather than raw text.
```shell
cargo install nu
```
See https://www.nushell.sh/ for more information.

#### Install Sqlite

On Ubuntu:
```shell
sudo apt update && sudo apt install sqlite3
```

## Running RoboCroissant Pipeline

This will run the pipeline for one knowledge base.  Note that the RCP_KB_NAME environment variable must be set to the name of the knowledge base from the knowledge_bases.toml file.  Here, CTD is specified as the knowledge base name.
```nushell
RCP_KB_NAME="CTD" opencode run --agent croissant-metadata-generator "run"
```

There is a convenience script called ./bin/run_main_agent.nu that will invoke the Croissant Metadata Generator Agent for all enabled Knowledge Bases. 
```nushell
nu ./bin/run_main_agent.nu
```

To generate the robo_croissant.db file, run the following command:
```nushell
nu ./bin/build_db.nu
```

## Extracting the Croissant Metadata JSON from the DB 
From within Nushell, issue the follow:
```shell
let kb_names = open knowledge_bases.toml | get knowledge_bases.name
for x in $kb_names { open robo_croissant.db | get knowledge_bases | where name == $x | save -f $"($x).json" }
```

The current list of generated Croissant Metadata JSON files can be found [here](https://roboboulangerie.github.io/robo-croissant-pipeline/).





