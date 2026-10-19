# Templum's shell integration for zsh, found through ZDOTDIR. It tells Templum when a prompt
# starts (OSC 133 A), when a command starts and ends (C and D, with the exit code) and the
# current folder (OSC 7), and changes nothing else.

# The user's ZDOTDIR back first: zsh reads .zprofile, .zshrc and .zlogin from it.
if (( ${+TEMPLUM_ZDOTDIR} )); then
    ZDOTDIR=$TEMPLUM_ZDOTDIR
    builtin unset TEMPLUM_ZDOTDIR
else
    builtin unset ZDOTDIR
fi
# The user's .zshenv, which may set ZDOTDIR itself.
[[ -r ${ZDOTDIR:-$HOME}/.zshenv ]] && builtin source ${ZDOTDIR:-$HOME}/.zshenv

if [[ -o interactive && -z ${__templum_installed-} ]]; then
    __templum_installed=1
    __templum_precmd() {
        builtin local ret=$?
        builtin printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' $ret "$HOST" "${PWD//\%/%25}"
        builtin return $ret
    }
    __templum_preexec() {
        builtin printf '\e]133;C\a'
    }
    builtin autoload -Uz add-zsh-hook
    # Before the .zshrc's hooks: the first precmd sees the command's status.
    add-zsh-hook precmd __templum_precmd
    add-zsh-hook preexec __templum_preexec

    # A `claude`, `codex` or `copilot` typed here gets Templum's MCP server and hooks: the words of
    # TEMPLUM_CLAUDE_ARGS, TEMPLUM_CODEX_ARGS or TEMPLUM_COPILOT_ARGS, one per line, before the
    # person's, unless they name Templum's already (docs/specs/connections.md, "Agents typed in a
    # terminal"). A function of that name in the .zshrc, read next, replaces it; `function` keeps an
    # alias of that name from breaking this.
    __templum_agent() {
        builtin local name="$1" w="$2"
        # A variable for this command only, unless the shell has one (the block Templum opened,
        # the person's own).
        if [[ -n "$4" ]]; then
            [[ -n "${(P)3-}" ]] || builtin local -x "$3=$4"
        fi
        builtin shift 4
        # As zsh says it, not from inside this function. A path search, as `command` does it:
        # $commands misses an agent installed after zsh read its folder.
        if ! builtin whence -p -- "$name" > /dev/null; then
            builtin print -u2 "zsh: command not found: $name"
            builtin return 127
        fi
        if [[ -z "$w" ]]; then
            command "$name" "$@"
        else
            command "$name" "${(@f)w}" "$@"
        fi
    }
    if [[ -n "${TEMPLUM_CLAUDE_ARGS-}" ]] && (( ! $+functions[claude] )); then
        function claude {
            builtin local w="${TEMPLUM_CLAUDE_ARGS-}" a
            for a in "$@"; do
                case "$a" in (--settings | --settings=* | --mcp-config | --mcp-config=*) w= ;; esac
            done
            __templum_agent claude "$w" "" "" "$@"
        }
    fi
    if [[ -n "${TEMPLUM_CODEX_ARGS-}" ]] && (( ! $+functions[codex] )); then
        function codex {
            builtin local w="${TEMPLUM_CODEX_ARGS-}" a
            for a in "$@"; do
                case "$a" in (mcp_servers.templum.*) w= ;; esac
            done
            __templum_agent codex "$w" "" "" "$@"
        }
    fi
    # The others' words end with a path in Templum's folder, which only Templum types: none when
    # the person's words hold it (docs/specs/connections.md, "Typed by hand: the shells").
    if [[ -n "${TEMPLUM_COPILOT_ARGS-}" ]] && (( ! $+functions[copilot] )); then
        function copilot {
            builtin local w="${TEMPLUM_COPILOT_ARGS-}" last a
            # Not before a subcommand (`copilot plugin list`) nor for the help or the version: AWS's
            # `copilot` takes those too, and refuses Templum's words.
            case "${1--}" in (-*) ;; (*) w= ;; esac
            last="${w##*$'\n'}"
            for a in "$@"; do
                case "$a" in ("$last" | -h | --help | -v | --version) w= ;; esac
            done
            __templum_agent copilot "$w" "" "" "$@"
        }
    fi
    # OpenCode and Kilo Code take Templum's server and plugin from a variable.
    if [[ -n "${TEMPLUM_OPENCODE_CONFIG-}" ]] && (( ! $+functions[opencode] )); then
        function opencode {
            __templum_agent opencode "" OPENCODE_CONFIG_CONTENT "${TEMPLUM_OPENCODE_CONFIG-}" "$@"
        }
    fi
    if [[ -n "${TEMPLUM_KILO_CONFIG-}" ]] && (( ! $+functions[kilo] )); then
        function kilo {
            __templum_agent kilo "" KILO_CONFIG_CONTENT "${TEMPLUM_KILO_CONFIG-}" "$@"
        }
    fi
    # Qwen Code: its server as a word, its hooks in the settings file its variable moves. It takes
    # one --mcp-config: a person's own means none of Templum's. None before one of its commands,
    # which refuse it (`qwen mcp list`); a plain word that isn't one is a prompt.
    if [[ -n "${TEMPLUM_QWEN_ARGS-}" ]] && (( ! $+functions[qwen] )); then
        function qwen {
            builtin local w="${TEMPLUM_QWEN_ARGS-}" a
            case "${1-}" in
                (agents | auth | batch | board | channel | extensions | hooks | mcp | review | sandbox | \
                    serve | sessions | update) w= ;;
            esac
            for a in "$@"; do
                case "$a" in (--mcp-config | --mcp-config=*) w= ;; esac
            done
            __templum_agent qwen "$w" QWEN_CODE_SYSTEM_SETTINGS_PATH "${TEMPLUM_QWEN_SETTINGS-}" "$@"
        }
    fi
    # Pi: its extension. Not before one of its commands, which it reads only as the first word.
    if [[ -n "${TEMPLUM_PI_ARGS-}" ]] && (( ! $+functions[pi] )); then
        function pi {
            builtin local w="${TEMPLUM_PI_ARGS-}" last a
            case "${1-}" in (install | remove | uninstall | update | list | config | auth) w= ;; esac
            last="${w##*$'\n'}"
            for a in "$@"; do
                case "$a" in ("$last") w= ;; esac
            done
            __templum_agent pi "$w" "" "" "$@"
        }
    fi
fi
