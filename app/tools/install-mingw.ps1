# Downloads a portable MinGW-w64 toolchain (WinLibs) so the Windows GNU build has a full dlltool + as.
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$dest = Join-Path $env:LOCALAPPDATA 'Programs\winlibs'
if (Test-Path (Join-Path $dest 'mingw64\bin\dlltool.exe')) { "already installed: $dest"; exit 0 }
$rel = Invoke-RestMethod -UseBasicParsing 'https://api.github.com/repos/brechtsanders/winlibs_mingw/releases/latest' -Headers @{ 'User-Agent' = 'build-script' }
$asset = $rel.assets | Where-Object { $_.name -match '^winlibs-x86_64-posix-seh-gcc-.*-mingw-w64ucrt-.*\.zip$' -and $_.name -notmatch 'llvm' } | Select-Object -First 1
if (-not $asset) { $asset = $rel.assets | Where-Object { $_.name -match '^winlibs-x86_64-posix-seh-.*ucrt.*\.zip$' } | Select-Object -First 1 }
"Downloading $($asset.name) ($([math]::Round($asset.size/1MB)) MB)"
$zip = Join-Path $env:TEMP $asset.name
$ProgressPreference = 'SilentlyContinue'
Invoke-WebRequest -UseBasicParsing $asset.browser_download_url -OutFile $zip
New-Item -ItemType Directory -Force $dest | Out-Null
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip -Force
Get-ChildItem (Join-Path $dest 'mingw64\bin') -Filter 'dlltool.exe' | Select-Object -ExpandProperty FullName
Get-ChildItem (Join-Path $dest 'mingw64\bin') -Filter 'as.exe' | Select-Object -ExpandProperty FullName
