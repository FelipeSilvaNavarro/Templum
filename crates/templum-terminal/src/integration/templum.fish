# Templum's shell integration for fish, started with `--init-command 'source …'` this file. It
# tells Templum when a prompt starts (OSC 133 A), when a command starts and ends (C and D, with
# the exit code) and the current folder (OSC 7), and changes nothing else.

if status is-interactive; and not set -q __templum_installed
    set -g __templum_installed 1
    function __templum_preexec --on-event fish_preexec
        printf '\e]133;C\a'
    end
    function __templum_postexec --on-event fish_postexec
        printf '\e]133;D;%s\a' $status
    end
    function __templum_prompt --on-event fish_prompt
        printf '\e]7;file://%s%s\a\e]133;A\a' $hostname (string replace -a '%' '%25' -- $PWD)
    end

    # A `claude` or `codex` typed here gets Templum's MCP server and hooks: the words of
    # TEMPLUM_CLAUDE_ARGS or TEMPLUM_CODEX_ARGS, one per line, before the person's, unless they
    # name Templum's already (docs/specs/connections.md, "Agents typed in a terminal"). Not over
    # the person's own function (an alias, an autoloaded one): fish read their config before this.
    if test -n "$TEMPLUM_CLAUDE_ARGS"; and not functions -q claude
        function claude
            set -l words (string split --no-empty \n -- "$TEMPLUM_CLAUDE_ARGS")
            for a in $argv
                switch $a
                    case --settings '--settings=*' --mcp-config '--mcp-config=*'
                        set words
                end
            end
            command claude $words $argv
        end
    end
    if test -n "$TEMPLUM_CODEX_ARGS"; and not functions -q codex
        function codex
            set -l words (string split --no-empty \n -- "$TEMPLUM_CODEX_ARGS")
            for a in $argv
                switch $a
                    case 'mcp_servers.templum.*'
                        set words
                end
            end
            command codex $words $argv
        end
    end
end
