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

    # A `claude` or `codex` typed here gets Templum's MCP server and hooks: the words of
    # TEMPLUM_CLAUDE_ARGS or TEMPLUM_CODEX_ARGS, one per line, before the person's, unless they
    # name Templum's already (docs/specs/connections.md, "Agents typed in a terminal"). A function
    # of that name in the .zshrc, read next, replaces it; `function` keeps an alias of that name
    # from breaking this.
    __templum_agent() {
        builtin local name="$1" w="$2"
        builtin shift 2
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
            __templum_agent claude "$w" "$@"
        }
    fi
    if [[ -n "${TEMPLUM_CODEX_ARGS-}" ]] && (( ! $+functions[codex] )); then
        function codex {
            builtin local w="${TEMPLUM_CODEX_ARGS-}" a
            for a in "$@"; do
                case "$a" in (mcp_servers.templum.*) w= ;; esac
            done
            __templum_agent codex "$w" "$@"
        }
    fi
fi
