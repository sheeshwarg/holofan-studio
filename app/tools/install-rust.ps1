# Installs the Rust toolchain (GNU host, needs no Visual Studio) for building this app.
$ErrorActionPreference = 'Stop'
$p = Join-Path $env:TEMP 'rustup-init.exe'
Invoke-WebRequest -UseBasicParsing 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $p
& $p -y --default-host x86_64-pc-windows-gnu --profile minimal --default-toolchain stable
& "$env:USERPROFILE\.cargo\bin\cargo.exe" --version
