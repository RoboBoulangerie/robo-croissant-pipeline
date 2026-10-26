#!/usr/bin/env nu

# Run one of the agents in ./agents under a coding-agent harness.
#
# The harness is chosen by --harness, then $env.RCP_HARNESS, then "opencode".
# The model is chosen by --model, then $env.RCP_MODEL; if neither is set the
# harness's own default model is used.
def main [
    agent?: string            # agent name, e.g. croissant-metadata-generator
    task: string = "run"      # user prompt passed to the agent
    --harness: string         # opencode | pi | claude
    --model: string           # model id in the harness's own format
    --dry-run                 # print the command instead of running it
] {
    let harness = $harness | default ($env.RCP_HARNESS? | default "opencode")
    let model = $model | default ($env.RCP_MODEL? | default "")
    let repo_root = $env.FILE_PWD | path dirname
    let agents_dir = $repo_root | path join "agents"

    if $agent == null {
        let names = ls ($agents_dir | path join "*.md" | into glob) | get name | path parse | get stem | sort
        print -e "Usage: nu ./bin/run_agent.nu <agent> [task] [--harness opencode|pi|claude] [--model <id>] [--dry-run]"
        print -e $"Available agents: ($names | str join ', ')"
        exit 2
    }

    let prompt_path = $agents_dir | path join $"($agent).md"

    if not ($prompt_path | path exists) {
        print -e $"Unknown agent '($agent)': ($prompt_path) not found"
        exit 2
    }

    let bin = match $harness {
        "opencode" => "opencode"
        "pi" => "pi"
        "claude" => "claude"
        _ => {
            print -e $"Unsupported harness '($harness)'; expected opencode, pi, or claude"
            exit 2
        }
    }

    if not $dry_run and (which $bin | is-empty) {
        print -e $"Harness '($harness)' selected but `($bin)` is not on PATH"
        exit 127
    }

    # OpenCode reads .opencode/agents/<agent>.md (a symlink into ./agents) itself;
    # the other harnesses get the prompt body appended to their system prompt as
    # inline text, so there is no temp file to clean up.
    let prompt = if $harness == "opencode" { "" } else { strip_frontmatter $prompt_path }

    let model_args = if ($model | is-empty) { [] } else { ["--model" $model] }
    let cmd_args = match $harness {
        "opencode" => (["run" "--agent" $agent] ++ $model_args ++ [$task])
        "pi" => (["-p" "--no-session" "--append-system-prompt" $prompt] ++ $model_args ++ [$task])
        "claude" => (["-p" "--append-system-prompt" $prompt "--permission-mode" "bypassPermissions"] ++ $model_args ++ [$task])
    }

    if $dry_run {
        let shown = $cmd_args | each {|arg| if $arg == $prompt and $prompt != "" { $"<body of agents/($agent).md>" } else { $arg } }
        print ([$bin] ++ $shown | str join " ")
        return
    }

    # Run the harness as the script's last command so its exit code becomes
    # this script's exit code (nu 0.99 resets $env.LAST_EXIT_CODE inside try/do).
    run-external $bin ...$cmd_args
}

# Return a markdown file's contents without a leading YAML frontmatter block.
def strip_frontmatter [path: path] {
    let lines = open --raw $path | lines
    if ($lines | first) != "---" {
        return ($lines | str join "\n")
    }
    let end = $lines | skip 1 | enumerate | where item == "---" | get index | first
    $lines | skip ($end + 2) | str join "\n"
}
