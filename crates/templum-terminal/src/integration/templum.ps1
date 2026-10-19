# Templum's shell integration for PowerShell (7 and Windows PowerShell 5.1), started with
# `-NoExit -Command` reading this file. It tells Templum when a prompt starts (OSC 133 A), when a
# command starts and ends (C and D, with the exit code) and the current folder (OSC 7):
# PowerShell's `cd` doesn't move its process, so Templum can't ask the system for it. It wraps
# the prompt and, with PSReadLine, reading the command line. Parts follow VS Code's
# shellIntegration.ps1 (MIT, (c) Microsoft).

if ($null -eq $Global:__TemplumPrompt) {
    $Global:__TemplumPrompt = $function:prompt
    $Global:__TemplumRunning = $false

    function Global:prompt {
        $ok = $global:?
        # An agent's function (below) leaves `$?` true whatever its program's exit code.
        if ($Global:__TemplumAgentExit) { $ok = $false }
        $Global:__TemplumAgentExit = $null
        Set-StrictMode -Off
        $code = $global:LASTEXITCODE
        $esc = [char]27
        $bel = [char]7
        $out = ''
        if ($Global:__TemplumRunning) {
            $Global:__TemplumRunning = $false
            $exit = if ($ok) { 0 } elseif ($code) { $code } else { 1 }
            $out += "$esc]133;D;$exit$bel"
        }
        if ($PWD.Provider.Name -eq 'FileSystem') {
            $dir = $PWD.ProviderPath.Replace('\', '/').Replace('%', '%25')
            if (-not $dir.StartsWith('/')) { $dir = "/$dir" }
            $out += "$esc]7;file://$([Environment]::MachineName)$dir$bel"
        }
        $out += "$esc]133;A$bel"
        # The prompt it wraps sees the command's status, as VS Code does it: $? is read-only.
        $global:LASTEXITCODE = $code
        if (-not $ok) { Write-Error 'failure' -ErrorAction Ignore }
        $out + $Global:__TemplumPrompt.Invoke()
    }

    if (Get-Module PSReadLine) {
        $Global:__TemplumReadLine = $function:PSConsoleHostReadLine
        function Global:PSConsoleHostReadLine {
            $line = [string]$Global:__TemplumReadLine.Invoke()
            if ($line.Trim()) {
                $Global:__TemplumRunning = $true
                [Console]::Write("$([char]27)]133;C$([char]7)")
            }
            $line
        }
    }

    # A `claude`, `codex` or `copilot` typed here gets Templum's MCP server and hooks: the words of
    # TEMPLUM_CLAUDE_ARGS, TEMPLUM_CODEX_ARGS or TEMPLUM_COPILOT_ARGS, one per line, before the
    # person's, unless they name Templum's already (docs/specs/connections.md, "Agents typed in a terminal"). Not over
    # the person's own function of that name; their alias comes before any function anyway. The
    # program is the first application or script of that name (npm's claude.ps1 included):
    # several on PATH make an array `&` can't run. What is piped in is passed on (`git diff |
    # claude -p review`), and only then: an `$input` always given takes the console from claude.
    function Global:__TemplumAgent($name, $words, $var, $value, $given) {
        $program = Get-Command $name -CommandType Application, ExternalScript -ErrorAction Ignore |
            Select-Object -First 1
        if (-not $program) {
            # As PowerShell says it, without this function's lines.
            $Host.UI.WriteErrorLine("${name}: The term '$name' is not recognized as a name of a " +
                'cmdlet, function, script file, or executable program.')
            $Global:__TemplumAgentExit = 1
            return
        }
        $words = @($words -split "`n" | Where-Object { $_ })
        # PowerShell makes an array of an unquoted a,b: a program gets it as one word, a,b.
        $given = @(foreach ($a in $given) { if ($a -is [array]) { $a -join ',' } else { $a } })
        # A variable for this command only, unless the shell has one (the block Templum opened,
        # the person's own); put back as it was after.
        $set = $value -and -not (Get-Item "Env:$var" -ErrorAction Ignore).Value
        if ($set) { Set-Item "Env:$var" $value }
        try {
            if ($MyInvocation.ExpectingInput) { $input | & $program @words @given }
            else { & $program @words @given }
        } finally {
            if ($set) { Remove-Item "Env:$var" -ErrorAction Ignore }
        }
        $Global:__TemplumAgentExit = $LASTEXITCODE
    }
    if ($env:TEMPLUM_CLAUDE_ARGS -and -not (Test-Path Function:\claude)) {
        function Global:claude {
            $words = $env:TEMPLUM_CLAUDE_ARGS
            foreach ($a in $args) {
                if ("$a" -in '--settings', '--mcp-config' -or "$a" -like '--settings=*' -or
                    "$a" -like '--mcp-config=*') { $words = '' }
            }
            if ($MyInvocation.ExpectingInput) { $input | __TemplumAgent claude $words '' '' $args }
            else { __TemplumAgent claude $words '' '' $args }
        }
    }
    if ($env:TEMPLUM_CODEX_ARGS -and -not (Test-Path Function:\codex)) {
        function Global:codex {
            $words = $env:TEMPLUM_CODEX_ARGS
            foreach ($a in $args) {
                if ("$a" -like 'mcp_servers.templum.*') { $words = '' }
            }
            if ($MyInvocation.ExpectingInput) { $input | __TemplumAgent codex $words '' '' $args }
            else { __TemplumAgent codex $words '' '' $args }
        }
    }
    # The others' words end with a path in Templum's folder, which only Templum types: none when
    # the person's words hold it (docs/specs/connections.md, "Typed by hand: the shells").
    if ($env:TEMPLUM_COPILOT_ARGS -and -not (Test-Path Function:\copilot)) {
        function Global:copilot {
            $words = $env:TEMPLUM_COPILOT_ARGS
            # Not before a subcommand (`copilot plugin list`) nor for the help or the version: AWS's
            # `copilot` takes those too, and refuses Templum's words.
            if ($args.Count -and -not "$($args[0])".StartsWith('-')) { $words = '' }
            $last = ($words -split "`n")[-1]
            $asks = '-h', '--help', '-v', '--version'
            foreach ($a in $args) { if ("$a" -eq $last -or "$a" -in $asks) { $words = '' } }
            if ($MyInvocation.ExpectingInput) { $input | __TemplumAgent copilot $words '' '' $args }
            else { __TemplumAgent copilot $words '' '' $args }
        }
    }
    # OpenCode and Kilo Code take Templum's server and plugin from a variable.
    if ($env:TEMPLUM_OPENCODE_CONFIG -and -not (Test-Path Function:\opencode)) {
        function Global:opencode {
            $config = $env:TEMPLUM_OPENCODE_CONFIG
            if ($MyInvocation.ExpectingInput) {
                $input | __TemplumAgent opencode '' OPENCODE_CONFIG_CONTENT $config $args
            } else { __TemplumAgent opencode '' OPENCODE_CONFIG_CONTENT $config $args }
        }
    }
    if ($env:TEMPLUM_KILO_CONFIG -and -not (Test-Path Function:\kilo)) {
        function Global:kilo {
            $config = $env:TEMPLUM_KILO_CONFIG
            if ($MyInvocation.ExpectingInput) {
                $input | __TemplumAgent kilo '' KILO_CONFIG_CONTENT $config $args
            } else { __TemplumAgent kilo '' KILO_CONFIG_CONTENT $config $args }
        }
    }
    # Qwen Code: its server as a word, its hooks in the settings file its variable moves. It takes
    # one --mcp-config: a person's own means none of Templum's. None before one of its commands,
    # which refuse it (`qwen mcp list`); a plain word that isn't one is a prompt.
    if ($env:TEMPLUM_QWEN_ARGS -and -not (Test-Path Function:\qwen)) {
        function Global:qwen {
            $words = $env:TEMPLUM_QWEN_ARGS
            $commands = 'agents', 'auth', 'batch', 'board', 'channel', 'extensions', 'hooks', 'mcp', 'review',
                'sandbox', 'serve', 'sessions', 'update'
            if ($args.Count -and "$($args[0])" -in $commands) { $words = '' }
            foreach ($a in $args) {
                if ("$a" -eq '--mcp-config' -or "$a" -like '--mcp-config=*') { $words = '' }
            }
            $settings = $env:TEMPLUM_QWEN_SETTINGS
            if ($MyInvocation.ExpectingInput) {
                $input | __TemplumAgent qwen $words QWEN_CODE_SYSTEM_SETTINGS_PATH $settings $args
            } else { __TemplumAgent qwen $words QWEN_CODE_SYSTEM_SETTINGS_PATH $settings $args }
        }
    }
    # Pi: its extension. Not before one of its commands, which it reads only as the first word.
    if ($env:TEMPLUM_PI_ARGS -and -not (Test-Path Function:\pi)) {
        function Global:pi {
            $words = $env:TEMPLUM_PI_ARGS
            $commands = 'install', 'remove', 'uninstall', 'update', 'list', 'config', 'auth'
            if ($args.Count -and "$($args[0])" -in $commands) { $words = '' }
            $last = ($words -split "`n")[-1]
            foreach ($a in $args) { if ("$a" -eq $last) { $words = '' } }
            if ($MyInvocation.ExpectingInput) { $input | __TemplumAgent pi $words '' '' $args }
            else { __TemplumAgent pi $words '' '' $args }
        }
    }
}
