# Install the dmsg CLI and daemon on Windows x64.
#
#   irm https://raw.githubusercontent.com/ldclabs/ic-panda/main/scripts/dmsg/install.ps1 | iex
#   & .\install.ps1 -Uninstall
#
# The binary comes from the latest dmsg-v* release and is checked against its
# published SHA-256, then `dmsg install` puts it in DMSG_INSTALL_DIR (default
# %LOCALAPPDATA%\dMsg\bin, which the user PATH gains), never over a newer one.
# Upgrade with `dmsg update`. Uninstalling stops the daemon, removes the login
# registration and the binary, and keeps the data in %USERPROFILE%\.dmsg.
#
# Environment: DMSG_VERSION, DMSG_INSTALL_DIR, DMSG_RELEASE_URL.

param([switch]$Uninstall)

$ErrorActionPreference = 'Stop'
$Releases = if ($env:DMSG_RELEASE_URL) { $env:DMSG_RELEASE_URL } else { 'https://github.com/ldclabs/ic-panda/releases' }
$InstallDir = if ($env:DMSG_INSTALL_DIR) { $env:DMSG_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'dMsg\bin' }
$Dmsg = Join-Path $InstallDir 'dmsg.exe'

function Set-UserPath([string[]]$Entries) {
  [Environment]::SetEnvironmentVariable('Path', ($Entries | Where-Object { $_ }) -join ';', 'User')
}

$UserPath = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';')

if ($Uninstall) {
  if (-not (Test-Path $Dmsg)) { throw "no dmsg at $Dmsg" }
  & $Dmsg stop 2>$null | Out-Null
  & $Dmsg autostart uninstall | Out-Null
  # A daemon that just exited may still hold the file for a moment.
  for ($i = 0; $i -lt 20 -and (Test-Path $Dmsg); $i++) {
    try { Remove-Item -Force $Dmsg } catch { Start-Sleep -Milliseconds 250 }
  }
  Get-ChildItem -Force $InstallDir -Filter '.dmsg.exe.old-*' -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
  Set-UserPath ($UserPath | Where-Object { $_ -ne $InstallDir })
  Write-Host "Removed $Dmsg and its login registration."
  Write-Host "Your data stays in $env:USERPROFILE\.dmsg; delete it yourself only after it has synced."
  return
}

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw "dMsg releases Windows binaries for x64 only" }
$Asset = 'dmsg-windows-x64.exe'
if ($env:DMSG_VERSION) {
  $Tag = 'dmsg-v' + $env:DMSG_VERSION.TrimStart('v')
} else {
  $Tag = (Invoke-RestMethod -Uri "$Releases/latest" -Headers @{ Accept = 'application/json' }).tag_name
  if ($Tag -notlike 'dmsg-v*') { throw "the latest release ($Tag) is not a dMsg client release" }
}

$Work = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
New-Item -ItemType Directory $Work | Out-Null
try {
  $Binary = Join-Path $Work 'dmsg.exe'
  Write-Host "Downloading $Asset $Tag..."
  Invoke-WebRequest -UseBasicParsing -Uri "$Releases/download/$Tag/$Asset" -OutFile $Binary
  $Listed = (Invoke-WebRequest -UseBasicParsing -Uri "$Releases/download/$Tag/$Asset.sha256").Content
  if ($Listed -is [byte[]]) { $Listed = [Text.Encoding]::UTF8.GetString($Listed) }
  $Expected = ($Listed.Trim() -split '\s+')[0].ToLowerInvariant()
  $Actual = (Get-FileHash -Algorithm SHA256 $Binary).Hash.ToLowerInvariant()
  if ($Expected -ne $Actual) { throw "$Asset does not match its published SHA-256" }
  & $Binary install --dir $InstallDir
  if ($LASTEXITCODE -ne 0) { throw "dmsg install failed" }
} finally {
  Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}

if ($UserPath -notcontains $InstallDir) {
  Set-UserPath ($UserPath + $InstallDir)
  Write-Host "Added $InstallDir to your user PATH; open a new terminal to use it."
}
Write-Host "Next: write $env:USERPROFILE\.dmsg\config.toml, then ``dmsg start`` and ``dmsg login``."
Write-Host "``dmsg autostart install`` starts the daemon when you log in."
