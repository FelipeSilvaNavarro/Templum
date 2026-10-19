# Install Templum on Windows, in PowerShell:
#
#   irm https://github.com/KingDasWinx/Templum/releases/latest/download/install.ps1 | iex
#
# Runs Templum's setup for you only, without asking for an administrator: Templum goes to %LOCALAPPDATA%\Templum,
# with a Start menu and a desktop shortcut, and updates itself from there.
#
#   $env:TEMPLUM_VERSION = "0.1.2"   before it, installs that version instead of the latest
#
# release.yml attaches this file to every release.

# A script block, so that its settings don't stay in the session iex runs it in.
& {
    $ErrorActionPreference = 'Stop'
    # Windows PowerShell's progress bar slows a download down many times over.
    $ProgressPreference = 'SilentlyContinue'
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $repo = 'KingDasWinx/Templum'

    $version = $env:TEMPLUM_VERSION
    if (-not $version) {
        $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
    }
    $version = $version.TrimStart('v')
    if ($version -notmatch '^\d+\.\d+\.\d+') { throw "templum: not a version: $version" }

    $asset = "templum_${version}_x64-setup.exe"
    $url = "https://github.com/$repo/releases/download/v$version/$asset"
    $setup = Join-Path ([IO.Path]::GetTempPath()) $asset
    try {
        Write-Host "templum: downloading $asset"
        Invoke-WebRequest $url -OutFile $setup -UseBasicParsing
        Invoke-WebRequest "$url.sha256" -OutFile "$setup.sha256" -UseBasicParsing
        $expected = (Get-Content "$setup.sha256" -Raw).Split(' ')[0]
        if ((Get-FileHash $setup -Algorithm SHA256).Hash -ne $expected) {
            throw "templum: $asset doesn't match its checksum; nothing was installed"
        }

        Write-Host "templum: installing Templum $version"
        # /S: silent. It closes a running Templum first.
        $process = Start-Process $setup -ArgumentList '/S' -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw "templum: the setup failed (exit code $($process.ExitCode))" }
        Write-Host "templum: installed Templum ${version}: open it from the Start menu"
    } finally {
        Remove-Item $setup, "$setup.sha256" -ErrorAction SilentlyContinue
    }
}
