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
# The binary name matches the package name for every crate in this repo today.
$BinName = $Package
$Target = "x86_64-pc-windows-msvc"
$Archive = "$BinName-$Target.zip"

function Resolve-LatestTag {
    param([string]$Package)
    # GitHub's /releases/latest shortcut is repo-wide, not per-package, so we
    # query the releases API and pick the newest tag matching "<package>/...".
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases?per_page=100"
    $match = $releases | Where-Object { $_.tag_name -like "$Package/*" } | Select-Object -First 1
    if (-not $match) {
        Write-Error "Could not find any release for package '$Package' in $Repo"
        exit 1
    }
    return $match.tag_name
}

if ($Version -eq "latest") {
    $Tag = Resolve-LatestTag -Package $Package
} elseif ($Version -like "*/*") {
    $Tag = $Version          # already a full tag, e.g. "knightwatch-cli/1.0.0"
} else {
    $Tag = "$Package/$Version"   # just a version number, e.g. "1.0.0"
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