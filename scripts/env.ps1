# Prepares this PowerShell session for building Open Headphones Connect.
#
#   . .\scripts\env.ps1        (note the leading dot)
#
# On a machine with a complete Windows SDK there is nothing to prepare and
# this only reports that. Two things can be changed per machine:
#
#   * $env:OHC_BUILD = "D:\somewhere" moves the build output off the system
#     drive (to D:\somewhere\target).
#   * When the SDK's libraries are missing, a portable copy unpacked into
#     <build folder>\winsdk (Microsoft.Windows.SDK.CPP and
#     Microsoft.Windows.SDK.CPP.x64 from nuget.org) is used instead and the
#     MSVC linker is wired up by hand.
#
# Put machine-specific settings such as OHC_BUILD in scripts\env.local.ps1,
# which is not committed.

$local = Join-Path $PSScriptRoot "env.local.ps1"
if (Test-Path $local) { . $local }

$root = Split-Path $PSScriptRoot -Parent
if ($env:OHC_BUILD) {
    $build = $env:OHC_BUILD
    $env:CARGO_TARGET_DIR = Join-Path $build "target"
    $output = $env:CARGO_TARGET_DIR
} else {
    $build = Join-Path $root ".build"
    $output = Join-Path $root "src-tauri\target"
}

$kits = "${env:ProgramFiles(x86)}\Windows Kits\10\Lib"
$systemSdk = Test-Path "$kits\*\um\x64\kernel32.lib"
if ($systemSdk) {
    Write-Host "Using the installed Windows SDK. Build output: $output"
    return
}

$sdk = Join-Path $build "winsdk"
$headers = Join-Path $sdk "microsoft.windows.sdk.cpp\c"
$libs = Join-Path $sdk "microsoft.windows.sdk.cpp.x64\c"
if (-not (Test-Path "$libs\um\x64\kernel32.lib")) {
    throw "No Windows SDK found. Install one with Visual Studio Installer, or unpack Microsoft.Windows.SDK.CPP and Microsoft.Windows.SDK.CPP.x64 into $sdk."
}

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Visual Studio C++ build tools were not found." }
$msvcVersion = (Get-Content "$vs\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt").Trim()
$msvc = "$vs\VC\Tools\MSVC\$msvcVersion"
$sdkVersion = (Get-ChildItem "$headers\Include" -Directory | Sort-Object Name | Select-Object -Last 1).Name

# Looks like a Developer Command Prompt to rustc and the `cc` crate, so they
# take the tools from PATH instead of searching for an installed SDK.
$env:VSINSTALLDIR = "$vs\"
$env:VCINSTALLDIR = "$vs\VC\"
$env:VCToolsInstallDir = "$msvc\"
$env:VSCMD_ARG_HOST_ARCH = "x64"
$env:VSCMD_ARG_TGT_ARCH = "x64"
$env:WindowsSdkDir = "$headers\"
$env:WindowsSDKVersion = "$sdkVersion\"

$env:LIB = "$msvc\lib\x64;$libs\um\x64;$libs\ucrt\x64"
$inc = "$headers\Include\$sdkVersion"
$env:INCLUDE = "$msvc\include;$inc\ucrt;$inc\um;$inc\shared;$inc\winrt;$inc\cppwinrt"

$toolPath = "$msvc\bin\Hostx64\x64;$headers\bin\$sdkVersion\x64"
if (-not $env:PATH.StartsWith($toolPath)) { $env:PATH = "$toolPath;$env:PATH" }

Write-Host "Using the portable Windows SDK $sdkVersion with MSVC $msvcVersion. Build output: $output"
