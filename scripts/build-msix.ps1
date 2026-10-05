<#
.SYNOPSIS
  Builds the Microsoft Store (MSIX) package of Matchstick from an already built executable.

.DESCRIPTION
  Lays out the files a package needs (the program, its logos, the manifest with
  the version filled in) and, if the Windows SDK's MakeAppx is available, packs
  them into Matchstick_<version>_x64.msix.

  The package is deliberately left unsigned: the Microsoft Store signs it after
  certification. To try the layout locally instead (Developer Mode on):
      Add-AppxPackage -Register <OutDir>\layout\AppxManifest.xml

.EXAMPLE
  scripts\build-msix.ps1 -Version 0.2.2 -Exe src-tauri\target\release\matchstick.exe -OutDir msix
#>
param(
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $OutDir,
    # Extra files that must sit next to the executable (none for the official MSVC build).
    [string[]] $Extra = @()
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)$') {
    throw "Version must be MAJOR.MINOR.PATCH, got '$Version'"
}
# The Store rejects versions that start with 0 and reserves the fourth part,
# so 0.2.2 is published as 1.2.2.0. (Same rule as store::package_version.)
$packageVersion = "{0}.{1}.{2}.0" -f ([int]$Matches[1] + 1), $Matches[2], $Matches[3]

$layout = Join-Path $OutDir "layout"
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $layout "Assets") | Out-Null

Copy-Item $Exe (Join-Path $layout "matchstick.exe")
foreach ($file in $Extra) { Copy-Item $file $layout }
foreach ($logo in "StoreLogo.png", "Square150x150Logo.png", "Square44x44Logo.png") {
    Copy-Item (Join-Path $root "src-tauri\icons\$logo") (Join-Path $layout "Assets\$logo")
}

$manifest = Get-Content (Join-Path $root "packaging\AppxManifest.xml") -Raw
$manifest.Replace("{VERSION}", $packageVersion) |
    Set-Content (Join-Path $layout "AppxManifest.xml") -Encoding utf8 -NoNewline

Write-Host "Layout ready: $layout (package version $packageVersion)"

$makeAppx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName | Select-Object -Last 1
if (-not $makeAppx) {
    Write-Warning "MakeAppx (Windows SDK) not found: the layout was created but not packed."
    return
}

$package = Join-Path $OutDir "Matchstick_${Version}_x64.msix"
& $makeAppx.FullName pack /d $layout /p $package /o
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE" }
Write-Host "Package ready: $package"
