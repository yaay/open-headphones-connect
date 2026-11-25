# Builds the MSIX package for the Microsoft Store.
#
#   .\packaging\build-msix.ps1            builds the app and the package
#   .\packaging\build-msix.ps1 -Register  also installs it on this PC for testing
#                                         (needs Developer Mode; nothing is signed)
#   .\packaging\build-msix.ps1 -Unregister  removes that test install again
#
#   -Arch x64 | arm64   which processors the package is for. Left out, it is
#                       what this PC's Rust builds by default.
#   -Exe <file>         packages a program that was built elsewhere (the ARM64
#                       one from the GitHub build, say) instead of building it.
#   -Bundle <folder>    builds nothing: puts the packages in that folder, one
#                       per architecture, into the single bundle the Store is
#                       sent (see .github\workflows\store.yml).
#
# The package is not signed: the Store signs it when it is published. The
# identity in the manifest has to match the one Partner Center gives the app,
# copied into packaging\store-identity.json (see store-identity.example.json).
# Without that file a test identity is used, which is fine for -Register but
# will be refused by the Store.

param(
    [ValidateSet("x64", "arm64")] [string]$Arch,
    [string]$Exe,
    [string]$Bundle,
    [switch]$Register, [switch]$Unregister, [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
. (Join-Path $root "scripts\env.ps1") | Out-Null
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "src-tauri\target" }

# MSIX versions have four parts, and the Store wants the last one to be 0.
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = "$($conf.version).0"
$displayName = $conf.productName

$makeappx = (Get-Command makeappx -ErrorAction SilentlyContinue).Source
if (-not $makeappx) {
    # Not on the path outside a developer prompt; it comes with the Windows SDK.
    $makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName | Select-Object -Last 1 -ExpandProperty FullName
}
if (-not $makeappx) { throw "makeappx.exe was not found. It comes with the Windows SDK." }

if ($Bundle) {
    $packages = @(Get-ChildItem $Bundle -Filter *.msix)
    if (-not $packages) { throw "No .msix packages in $Bundle." }
    # Named one by one, so that a bundle left by an earlier run is not swept in.
    $list = Join-Path ([IO.Path]::GetTempPath()) "ohc-bundle-$PID.txt"
    Set-Content $list (@("[Files]") + ($packages | ForEach-Object { "`"$($_.FullName)`" `"$($_.Name)`"" }))
    $bundled = Join-Path (Resolve-Path $Bundle) "OpenHeadphonesConnect-$($conf.version).msixbundle"
    & $makeappx bundle /o /f $list /p $bundled /bv $version | Out-Null
    $made = $LASTEXITCODE -eq 0
    Remove-Item $list
    if (-not $made) { throw "makeappx could not build the bundle." }
    Write-Host "Bundle: $bundled (version $version; $($packages.Name -join ', '))"
    return
}

# Rust builds for its own host unless told otherwise, and puts anything else
# in a folder named after the target.
$triples = @{ x64 = "x86_64-pc-windows-msvc"; arm64 = "aarch64-pc-windows-msvc" }
$hostTriple = (rustc -vV | Select-String "^host: ").Line -replace "^host: ", ""
if (-not $Arch) { $Arch = if ($hostTriple -like "aarch64-*") { "arm64" } else { "x64" } }
$triple = $triples[$Arch]
$foreign = $triple -ne $hostTriple

$identityFile = Join-Path $PSScriptRoot "store-identity.json"
$identity = if (Test-Path $identityFile) {
    Get-Content $identityFile -Raw | ConvertFrom-Json
} else {
    Write-Warning "No packaging\store-identity.json: using a test identity the Store will not accept."
    [pscustomobject]@{
        IdentityName = "OpenHeadphonesConnect.Test"
        Publisher = "CN=OpenHeadphonesConnectTest"
        PublisherDisplayName = "Open Headphones Connect contributors"
    }
}

if ($Unregister) {
    Get-AppxPackage -Name $identity.IdentityName | Remove-AppxPackage
    Write-Host "Removed the test install of $($identity.IdentityName)."
    return
}

if ($Register -and $foreign) {
    throw "A package for $Arch cannot be installed on this PC, which builds for $hostTriple."
}

if ($Exe) {
    $exe = (Resolve-Path $Exe).Path
} else {
    if (-not $SkipBuild) {
        # A shell prepared for one architecture links against that one's libraries.
        if ($foreign -and $env:VSCMD_ARG_TGT_ARCH -and $env:VSCMD_ARG_TGT_ARCH -ne $Arch) {
            throw "This shell is set up to build for $($env:VSCMD_ARG_TGT_ARCH), not $Arch. Build on a machine " +
                "with the MSVC $Arch build tools, or pass a program built elsewhere with -Exe."
        }
        $tauri = @("tauri", "build", "--no-bundle")
        if ($foreign) { $tauri += @("--target", $triple) }
        Push-Location $root
        try {
            npx @tauri
            if ($LASTEXITCODE -ne 0) { throw "The app did not build." }
        } finally { Pop-Location }
    }
    $release = if ($foreign) { "$triple\release" } else { "release" }
    $exe = Join-Path $target "$release\open-headphones-connect.exe"
    if (-not (Test-Path $exe)) { throw "Not found: $exe" }
}

# Lay the package out: the program, the tile and Store images, the manifest.
$out = Join-Path $target "msix"
$layout = Join-Path $out "layout-$Arch"
if (Test-Path $layout) { Remove-Item $layout -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $layout "Assets") | Out-Null
Copy-Item $exe $layout
Copy-Item (Join-Path $root "LICENSE") (Join-Path $layout "LICENSE.txt")
$icons = Join-Path $root "src-tauri\icons"
foreach ($name in "Square44x44Logo.png", "Square150x150Logo.png", "StoreLogo.png") {
    Copy-Item (Join-Path $icons $name) (Join-Path $layout "Assets")
}

$escape = { param($text) [System.Security.SecurityElement]::Escape($text) }
$manifest = Get-Content (Join-Path $PSScriptRoot "AppxManifest.xml") -Raw
$manifest = $manifest.Replace("{{IdentityName}}", (& $escape $identity.IdentityName)).
    Replace("{{Publisher}}", (& $escape $identity.Publisher)).
    Replace("{{PublisherDisplayName}}", (& $escape $identity.PublisherDisplayName)).
    Replace("{{DisplayName}}", (& $escape $displayName)).
    Replace("{{Version}}", $version).
    Replace("{{Arch}}", $Arch)
Set-Content (Join-Path $layout "AppxManifest.xml") $manifest -Encoding UTF8 -NoNewline

$package = Join-Path $out "OpenHeadphonesConnect-$($conf.version)-$Arch.msix"
& $makeappx pack /o /d $layout /p $package | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makeappx could not build the package." }
Write-Host "Package: $package ($([math]::Round((Get-Item $package).Length / 1MB, 2)) MB, version $version, $Arch)"

if ($Register) {
    # Installs straight from the folder, which Windows allows unsigned in Developer Mode.
    Get-AppxPackage -Name $identity.IdentityName | Remove-AppxPackage
    Add-AppxPackage -Register (Join-Path $layout "AppxManifest.xml")
    Write-Host "Installed for testing. Start it from the Start menu; remove it with -Unregister."
}
