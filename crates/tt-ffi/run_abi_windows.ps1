# Build and drive the ABI with the native MSVC toolchain. Activate the newest
# installed Visual Studio toolchain when this is an ordinary PowerShell rather
# than a Developer PowerShell.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path $vswhere)) { throw "cl.exe is not on PATH and vswhere.exe was not found" }
    $vs = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
    if (-not $vs) { throw "no Visual Studio C++ toolchain was found" }
    Import-Module (Join-Path $vs "Common7\Tools\Microsoft.VisualStudio.DevShell.dll")
    Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=x64 -host_arch=x64" | Out-Null
}

# `debug` is the *directory* a dev build lands in and is not a profile name:
# `cargo build --profile debug` is an error — "profile name `debug` is
# reserved" — so the flag goes in only when the caller asked for a profile,
# which is what `run_abi.sh` does. And this is deliberately not `$profile`:
# that is one of PowerShell's automatic variables, holding the path to the
# user's own profile script.
$outDir = if ($env:PROFILE) { $env:PROFILE } else { "debug" }
$target = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $PSScriptRoot "..\target"
}
$lib = Join-Path $target $outDir
$dll = Join-Path $lib "sterna.dll"
$import = Join-Path $lib "sterna.dll.lib"
$abi = Join-Path $lib "abi-windows.exe"
$compat = Join-Path $lib "abi-windows-compat.exe"

if ($env:PROFILE) {
    cargo build -p tt-ffi --profile $env:PROFILE
} else {
    cargo build -p tt-ffi
}
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if (-not (Test-Path $dll)) { throw "no $dll" }
if (-not (Test-Path $import)) { throw "no $import" }

# A warning in the generated header is the header's bug. /utf-8 is also part
# of the contract: the ABI and the one source literal below are UTF-8.
cl.exe /nologo /std:c11 /W4 /WX /utf-8 /Iinclude tests\abi_windows.c "/Fe:$abi" /link $import
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cl.exe /nologo /std:c++17 /EHsc /W4 /WX /utf-8 /Iinclude tests\abi_windows_compat.cpp "/Fe:$compat" /link $import
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

& $compat
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& $abi
exit $LASTEXITCODE
