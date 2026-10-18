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

    # A `claude`, `codex` or `copilot` typed here gets Templum's MCP server and hooks: the words
    # of TEMPLUM_CLAUDE_ARGS, TEMPLUM_CODEX_ARGS or TEMPLUM_COPILOT_ARGS, one per line, before the
    # person's, unless they name Templum's already (docs/specs/connections.md, "Agents typed in a
    # terminal"). Not over the person's own function (an alias, an autoloaded one): fish read
    # their config before this.
    function __templum_agent --argument-names name words var value
        # As fish says it, not from inside this function (before 3.2, without its handler).
        if not command -q $name
            if functions -q fish_command_not_found
                fish_command_not_found $name
            else
                echo "fish: Unknown command: $name" >&2
            end
            return 127
        end
        # A variable for this command only, unless the shell has one (the block Templum opened,
        # the person's own). Not in a block: `set -l` there would end with it.
        test -n "$value"; and test -z "$$var"; and set -lx $var $value
        command $name (string split --no-empty \n -- $words) $argv[5..]
    end
    if test -n "$TEMPLUM_CLAUDE_ARGS"; and not functions -q claude
        function claude
            set -l w "$TEMPLUM_CLAUDE_ARGS"
            for a in $argv
                switch $a
                    case --settings '--settings=*' --mcp-config '--mcp-config=*'
                        set w ''
                end
            end
            __templum_agent claude "$w" '' '' $argv
        end
    end
    if test -n "$TEMPLUM_CODEX_ARGS"; and not functions -q codex
        function codex
            set -l w "$TEMPLUM_CODEX_ARGS"
            for a in $argv
                switch $a
                    case 'mcp_servers.templum.*'
                        set w ''
                end
            end
            __templum_agent codex "$w" '' '' $argv
        end
    end
    # The others' words end with a path in Templum's folder, which only Templum types: none when
    # the person's words hold it (docs/specs/connections.md, "Typed by hand: the shells").
    if test -n "$TEMPLUM_COPILOT_ARGS"; and not functions -q copilot
        function copilot
            set -l w "$TEMPLUM_COPILOT_ARGS"
            # Not before a subcommand (`copilot plugin list`; AWS's `copilot` too).
            if set -q argv[1]; and not string match -q -- '-*' $argv[1]
                set w ''
            end
            set -l last (string split \n -- "$w")[-1]
            for a in $argv
                if test "$a" = "$last"
                    set w ''
                end
            end
            __templum_agent copilot "$w" '' '' $argv
        end
    end
    # OpenCode and Kilo Code take Templum's server and plugin from a variable.
    if test -n "$TEMPLUM_OPENCODE_CONFIG"; and not functions -q opencode
        function opencode
            __templum_agent opencode '' OPENCODE_CONFIG_CONTENT "$TEMPLUM_OPENCODE_CONFIG" $argv
        end
    end
    if test -n "$TEMPLUM_KILO_CONFIG"; and not functions -q kilo
        function kilo
            __templum_agent kilo '' KILO_CONFIG_CONTENT "$TEMPLUM_KILO_CONFIG" $argv
        end
    end
    # Qwen Code: its server as a word, its hooks in the settings file its variable moves. It takes
    # one --mcp-config: a person's own means none of Templum's.
    if test -n "$TEMPLUM_QWEN_ARGS"; and not functions -q qwen
        function qwen
            set -l w "$TEMPLUM_QWEN_ARGS"
            for a in $argv
                switch $a
                    case --mcp-config '--mcp-config=*'
                        set w ''
                end
            end
            __templum_agent qwen "$w" QWEN_CODE_SYSTEM_SETTINGS_PATH "$TEMPLUM_QWEN_SETTINGS" $argv
        end
    end
    # Pi: its extension. Not before one of its commands, which it reads only as the first word.
    if test -n "$TEMPLUM_PI_ARGS"; and not functions -q pi
        function pi
            set -l w "$TEMPLUM_PI_ARGS"
            if set -q argv[1]; and contains -- $argv[1] install remove uninstall update list config auth
                set w ''
            end
            set -l last (string split \n -- "$w")[-1]
            for a in $argv
                if test "$a" = "$last"
                    set w ''
                end
            end
            __templum_agent pi "$w" '' '' $argv
        end
    end
end
