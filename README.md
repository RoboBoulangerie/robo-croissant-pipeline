# Welcome to Robo Croissant Pipeline

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

### Install Nushell, AIChat, and Spider
AIChat is an all-in-one LLM CLI tool featuring Shell Assistant, CMD & REPL Mode, RAG, AI Tools & Agents, and More.
```shell
cargo install aichat
```
See https://github.com/sigoden/aichat for more information.

### Install Spider
Spider is a web crawler and scraper.
```shell
cargo install spider
```
See https://github.com/spider-rs/spider for more information.


## Running Robo Croissant Pipeline
```shell
nu main.nu
```

This will produce a Sqlite3 database file called `robo_croissant.db`.  This is the main database for the Robo Croissant Web Application.