# Installer for the knightwatch family of tools.
# Usage:
#   irm https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.ps1 | iex
# To pass parameters (pick a package/version), download then run it directly,
# since piping into `iex` can't forward -Package/-Version arguments:
#   iwr -useb https://raw.githubusercontent.com/YofaGh/knightwatch/master/scripts/install.ps1 -OutFile install.ps1
#   .\install.ps1 -Package knightwatch-cli -Version 1.0.0
param(
    [string]$Package = "knightwatch",
    [string]$Version = "latest",
    [string]$InstallDir = "$env:USERPROFILE\.cargo\bin"
)

$Repo = "YofaGh/knightwatch"

# --- Package/tag/binary naming ------------------------------------------
#
# Three names are involved for each crate in this repo, and they are NOT
# guaranteed to be the same string:
#
#   1. CRATE NAME  - what the crate is called in Cargo.toml. This is also
#                     the prefix used in git tags, e.g. "knightwatch-cli/1.0.1".
#                     Release resolution (Resolve-LatestTag below) always
#                     keys off the crate name, because that's what the tags
#                     use.
#   2. BinName     - the actual binary produced by the crate, and the
#                     prefix of the release asset filenames, e.g.
#                     "kwctl-x86_64-pc-windows-msvc.zip".
#   3. -Package    - what the *user* passes on the command line. We accept
#                     either the crate name or the binary name here for
#                     convenience, since users often only know the binary
#                     they run day to day.
#
# When a crate's binary name matches its crate name, no entry below is
# needed (see the default branch). Only add a mapping here when a crate's
# binary name diverges from its crate name - as knightwatch-cli/kwctl does.
#
# To add a new crate whose binary name differs from its crate name:
#   1. Add a line mapping the crate name -> its binary name in Resolve-BinName.
#   2. Add a line mapping the binary name -> the same binary name, so users
#      can pass either one via -Package.
function Resolve-BinName {
    param([string]$Package)
    switch ($Package) {
        "knightwatch-cli" { return "kwctl" }
        "kwctl"           { return "kwctl" }
        default           { return $Package }
    }
}

# Tags are keyed by crate name. Since a user might pass either the crate
# name or the binary name via -Package, normalize back to the crate name
# for tag lookups.
function Resolve-CrateName {
    param([string]$Package)
    switch ($Package) {
        "kwctl"  { return "knightwatch-cli" }
        default  { return $Package }
    }
}

$CrateName = Resolve-CrateName -Package $Package
$BinName = Resolve-BinName -Package $Package
# -------------------------------------------------------------------------

$Target = "x86_64-pc-windows-msvc"
$Archive = "$BinName-$Target.zip"

function Resolve-LatestTag {
    param([string]$CrateName)
    # GitHub's /releases/latest shortcut is repo-wide, not per-crate, so we
    # query the releases API and pick the newest tag matching "<crate>/...".
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=100"
    $match = $releases | Where-Object { $_.tag_name -like "$CrateName/*" } | Select-Object -First 1
    if (-not $match) {
        Write-Error "Could not find any release for package '$Package' in $Repo"
        exit 1
    }
    return $match.tag_name
}

if ($Version -eq "latest") {
    $Tag = Resolve-LatestTag -CrateName $CrateName
} elseif ($Version -like "*/*") {
    $Tag = $Version              # already a full tag, e.g. "knightwatch-cli/1.0.0"
} else {
    $Tag = "$CrateName/$Version" # just a version number, e.g. "1.0.0"
}

$Url = "https://github.com/$Repo/releases/download/$Tag/$Archive"

$TmpDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([System.Guid]::NewGuid()))
$ArchivePath = Join-Path $TmpDir $Archive

Write-Host "Downloading $Url"
Invoke-WebRequest -Uri $Url -OutFile $ArchivePath

Expand-Archive -Path $ArchivePath -DestinationPath $TmpDir

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item (Join-Path $TmpDir "$BinName-$Target\$BinName.exe") (Join-Path $InstallDir "$BinName.exe") -Force

Remove-Item -Recurse -Force $TmpDir

Write-Host "Installed $BinName ($Tag) to $InstallDir\$BinName.exe"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to your user PATH. Restart your terminal to pick it up."
}