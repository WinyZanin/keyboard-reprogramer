<#
.SYNOPSIS
    Builds the release exe and packages it: portable zip, Inno Setup installer, MSI and SHA256SUMS.

.DESCRIPTION
    The version comes from Cargo.toml (the single source of truth). Output goes to dist\.
    Used both locally and by .github/workflows/release.yml.

    Required tools:
      Inno Setup 6   winget install JRSoftware.InnoSetup
      WiX 5          dotnet tool install --global wix --version 5.0.2
                     wix extension add --global WixToolset.UI.wixext/5.0.2

.PARAMETER SkipBuild
    Package the existing target\release exe instead of running cargo build.

.PARAMETER SkipInno
    Don't build the Inno Setup installer.

.PARAMETER SkipMsi
    Don't build the MSI.
#>
[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$SkipInno,
    [switch]$SkipMsi
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Invoke-Native {
    param([string]$Exe, [string[]]$Arguments)
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe failed with exit code $LASTEXITCODE" }
}

$metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$package = $metadata.packages | Where-Object { $_.name -eq 'keyboard-reprogramer' }
$version = $package.version
Write-Host "Packaging keyboard-reprogramer $version"

if (-not $SkipBuild) {
    Invoke-Native cargo @('build', '--release', '--locked')
}
$exe = Join-Path $root 'target\release\keyboard-reprogramer.exe'
if (-not (Test-Path $exe)) { throw "$exe not found" }

$dist = Join-Path $root 'dist'
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory $dist | Out-Null
$base = "keyboard-reprogramer-$version-x64"

# Portable zip
$staging = Join-Path $dist 'portable'
New-Item -ItemType Directory $staging | Out-Null
Copy-Item $exe, 'README.md', 'README.pt-BR.md', 'CHANGELOG.md' $staging
Copy-Item 'LICENSE' (Join-Path $staging 'LICENSE.txt')
Compress-Archive -Path (Join-Path $staging '*') -DestinationPath (Join-Path $dist "$base-portable.zip")
Remove-Item -Recurse -Force $staging

# Inno Setup installer (writes dist\<base>-setup.exe itself, see OutputDir in the script)
if (-not $SkipInno) {
    $iscc = Get-Command iscc.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
    if (-not $iscc) {
        $iscc = @(
            "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
            "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
            "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
        ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    }
    if (-not $iscc) { throw 'Inno Setup 6 not found (winget install JRSoftware.InnoSetup), or use -SkipInno' }
    Invoke-Native $iscc @('/Q', "/DAppVersion=$version", 'installer\inno\keyboard-reprogramer.iss')
}

# MSI
if (-not $SkipMsi) {
    if (-not (Get-Command wix -ErrorAction SilentlyContinue)) {
        throw 'WiX not found (dotnet tool install --global wix --version 5.0.2), or use -SkipMsi'
    }
    Invoke-Native wix @(
        'build', 'installer\wix\Package.wxs',
        '-arch', 'x64',
        '-ext', 'WixToolset.UI.wixext',
        '-d', "Version=$version",
        '-o', (Join-Path $dist "$base.msi")
    )
    # wix also writes a .wixpdb debug file next to the msi; it isn't a release artifact.
    Remove-Item (Join-Path $dist '*.wixpdb') -ErrorAction SilentlyContinue
}

# Checksums (sha256sum format, so `sha256sum -c SHA256SUMS.txt` works)
$sums = Get-ChildItem $dist -File | Sort-Object Name | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
[System.IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'), [string[]]$sums)

Write-Host "`nArtifacts in dist\:"
Get-ChildItem $dist -File | ForEach-Object { Write-Host "  $($_.Name)" }
