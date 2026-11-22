# Builds release binaries and produces dist\RuskWatcher-Setup-<ver>.exe (NSIS) and dist\RuskWatcher-<ver>.msi (WiX 3).
# Build tools (NSIS, WiX) are downloaded once into %LOCALAPPDATA%\RuskWatcherBuildTools.
# Usage: powershell -ExecutionPolicy Bypass -File installer\build.ps1 [-SkipBuild]
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$tools = Join-Path $env:LOCALAPPDATA 'RuskWatcherBuildTools'
$version = ((Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value)
$icon = Join-Path $root 'crates\app\icons\icon.ico'
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist 'stage'

function Get-Tool($name, $url, $probe) {
    $dir = Join-Path $tools $name
    if (-not (Get-ChildItem $dir -Recurse -Filter $probe -ErrorAction SilentlyContinue)) {
        Write-Host "Downloading $name..."
        New-Item -ItemType Directory -Force $dir | Out-Null
        $zip = Join-Path $tools "$name.zip"
        Invoke-WebRequest $url -OutFile $zip -UseBasicParsing
        Expand-Archive $zip $dir -Force
        Remove-Item $zip
    }
    (Get-ChildItem $dir -Recurse -Filter $probe | Select-Object -First 1).DirectoryName
}

if (-not $SkipBuild) {
    Push-Location $root
    cargo build --release -p service -p ruskwatcher-app
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    Pop-Location
}

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item (Join-Path $root 'target\release\ruskwatcher-app.exe') (Join-Path $stage 'RuskWatcher.exe')
Copy-Item (Join-Path $root 'target\release\service.exe') (Join-Path $stage 'service.exe')

# --- NSIS .exe ---
$nsisDir = Get-Tool 'nsis' 'https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip' 'makensis.exe'
$setup = Join-Path $dist "RuskWatcher-Setup-$version.exe"
& (Join-Path $nsisDir 'makensis.exe') /V2 "/DVERSION=$version" "/DSRC=$stage" "/DOUT=$setup" "/DICON=$icon" (Join-Path $PSScriptRoot 'ruskwatcher.nsi')
if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }

# --- WiX .msi ---
$wixDir = Get-Tool 'wix' 'https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip' 'candle.exe'
$obj = Join-Path $dist 'ruskwatcher.wixobj'
$msi = Join-Path $dist "RuskWatcher-$version.msi"
& (Join-Path $wixDir 'candle.exe') -nologo -arch x64 "-dVersion=$version" "-dSrcDir=$stage" "-dIconFile=$icon" -ext WixUIExtension -out $obj (Join-Path $PSScriptRoot 'ruskwatcher.wxs')
if ($LASTEXITCODE -ne 0) { throw 'candle failed' }
& (Join-Path $wixDir 'light.exe') -nologo -ext WixUIExtension -sice:ICE61 -out $msi $obj
if ($LASTEXITCODE -ne 0) { throw 'light failed' }
Remove-Item $obj -Force

Write-Host "Built:`n  $setup`n  $msi"
