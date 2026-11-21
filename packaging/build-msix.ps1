# Builds the MSIX package for the Microsoft Store.
#
#   .\packaging\build-msix.ps1            builds the app and the package
#   .\packaging\build-msix.ps1 -Register  also installs it on this PC for testing
#                                         (needs Developer Mode; nothing is signed)
#   .\packaging\build-msix.ps1 -Unregister  removes that test install again
#
# The package is not signed: the Store signs it when it is published. The
# identity in the manifest has to match the one Partner Center gives the app,
# copied into packaging\store-identity.json (see store-identity.example.json).
# Without that file a test identity is used, which is fine for -Register but
# will be refused by the Store.

param([switch]$Register, [switch]$Unregister, [switch]$SkipBuild)
$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
. (Join-Path $root "scripts\env.ps1") | Out-Null
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "src-tauri\target" }

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

# MSIX versions have four parts, and the Store wants the last one to be 0.
$conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
$version = "$($conf.version).0"
$displayName = $conf.productName

if (-not $SkipBuild) {
    Push-Location $root
    try {
        npx tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "The app did not build." }
    } finally { Pop-Location }
}

$exe = Join-Path $target "release\open-headphones-connect.exe"
if (-not (Test-Path $exe)) { throw "Not found: $exe" }

# Lay the package out: the program, the tile and Store images, the manifest.
$out = Join-Path $target "msix"
$layout = Join-Path $out "layout"
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
    Replace("{{Version}}", $version)
Set-Content (Join-Path $layout "AppxManifest.xml") $manifest -Encoding UTF8 -NoNewline

$package = Join-Path $out "OpenHeadphonesConnect-$($conf.version)-x64.msix"
makeappx pack /o /d $layout /p $package | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makeappx could not build the package." }
Write-Host "Package: $package ($([math]::Round((Get-Item $package).Length / 1MB, 2)) MB, version $version)"

if ($Register) {
    # Installs straight from the folder, which Windows allows unsigned in Developer Mode.
    Get-AppxPackage -Name $identity.IdentityName | Remove-AppxPackage
    Add-AppxPackage -Register (Join-Path $layout "AppxManifest.xml")
    Write-Host "Installed for testing. Start it from the Start menu; remove it with -Unregister."
}
