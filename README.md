# Welcome to RoboCroissant Pipeline

## Getting started

### Install Rust

From https://rust-lang.org/tools/install/
```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Install Nushell
Nushell is a modern, cross-platform shell and programming language written in Rust that treats all data as structured tables rather than raw text.
```shell
cargo install nu
```
See https://www.nushell.sh/ for more information.

### Install and configure AIChat
AIChat is an all-in-one LLM CLI tool featuring Shell Assistant, CMD & REPL Mode, RAG, AI Tools & Agents, and More.
```shell
cargo install aichat
```
Once installed, call aichat, which will prompt you to create its configuration file
```shell
aichat hi
```
See https://github.com/sigoden/aichat for more information.

### Install Spider
Spider is a web crawler and scraper.
```shell
cargo install -F chrome -F regex spider_cli
```
See https://github.com/spider-rs/spider for more information.

## Running RoboCroissant Pipeline
This will produce a Sqlite3 database file called `robo_croissant.db` and is the database intended to be used for the RoboCroissant Dashboard.
```shell
nu main.nu
```

## Extracting the Croissant Metadata JSON 
From within Nushell, issue the follow:
```shell
let kb_names = open config.toml | get knowledge_sources | get name
for x in $kb_names { open robo_croissant.db | get knowledge_sources | where name == $x | save -f $"($x).json" }
```

The current list of generated Croissant Metadata JSON files can be found [here](https://roboboulangerie.github.io/robo-croissant-pipeline/).





