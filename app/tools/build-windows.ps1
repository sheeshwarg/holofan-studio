# Builds the finished Windows program and puts it in dist\HoloFan Studio.exe.
# Run from the app folder:  powershell -ExecutionPolicy Bypass -File tools\build-windows.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }
& $cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target\release\holofan_studio.exe 'dist\HoloFan Studio.exe' -Force
"Done: $(Resolve-Path 'dist\HoloFan Studio.exe')"
