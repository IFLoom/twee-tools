# Installs twee-tools on Windows, or upgrades it: downloads the release, checks it against the release's checksums,
# puts twee-tools.exe in %LOCALAPPDATA%\Programs\twee-tools and adds that folder to your PATH.
#
#   irm https://github.com/IFLoom/twee-tools/releases/latest/download/install.ps1 | iex
#
# Settings, as environment variables:
#   TWEE_TOOLS_VERSION       a version to install, such as 0.2.0 (default: the latest)
#   TWEE_TOOLS_INSTALL_DIR   where to put it (default: %LOCALAPPDATA%\Programs\twee-tools)
#   TWEE_TOOLS_DOWNLOAD_URL  where the release's files are (default: the GitHub release)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = 'https://github.com/IFLoom/twee-tools'
$version = "$env:TWEE_TOOLS_VERSION".TrimStart('v')
$dir = if ($env:TWEE_TOOLS_INSTALL_DIR) { $env:TWEE_TOOLS_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\twee-tools' }
# One build, for x64; Windows on Arm runs it too.
$target = 'x86_64-pc-windows-msvc'
$archive = "twee-tools-$target.zip"

$base = if ($env:TWEE_TOOLS_DOWNLOAD_URL) { $env:TWEE_TOOLS_DOWNLOAD_URL.TrimEnd('/') }
        elseif ($version) { "$repo/releases/download/v$version" }
        else { "$repo/releases/latest/download" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory $tmp | Out-Null
try {
    $label = if ($version) { $version } else { '(the latest)' }
    Write-Host "Downloading twee-tools $label for Windows..."
    Invoke-WebRequest "$base/$archive" -OutFile (Join-Path $tmp $archive) -UseBasicParsing
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS') -UseBasicParsing

    $line = Get-Content (Join-Path $tmp 'SHA256SUMS') | Where-Object { ($_ -split '\s+\*?')[1] -eq $archive } | Select-Object -First 1
    if (-not $line) { throw "the release's SHA256SUMS doesn't list $archive" }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash (Join-Path $tmp $archive) -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "$archive doesn't match its checksum: the download is damaged or not the release's" }

    Expand-Archive (Join-Path $tmp $archive) -DestinationPath $tmp
    New-Item -ItemType Directory $dir -Force | Out-Null
    $exe = Join-Path $dir 'twee-tools.exe'
    # A running twee-tools.exe can't be overwritten, but it can be renamed out of the way.
    if (Test-Path $exe) { Move-Item $exe "$exe.old" -Force }
    Copy-Item (Join-Path $tmp "twee-tools-$target\twee-tools.exe") $exe
    Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue

    Write-Host "Installed $(& $exe --version) in $dir."
    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($path -split ';') -notcontains $dir) {
        [Environment]::SetEnvironmentVariable('Path', ($(if ($path) { "$path;" }) + $dir), 'User')
        Write-Host "Added $dir to your PATH: open a new terminal to use twee-tools."
    }
    Write-Host 'Upgrade later with: twee-tools upgrade'
}
catch {
    # Not `exit`: run through `irm … | iex`, that would close the PowerShell window too.
    Write-Host "twee-tools install: $_" -ForegroundColor Red
    $global:LASTEXITCODE = 1
    return
}
finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
