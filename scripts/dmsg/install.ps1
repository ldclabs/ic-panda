# Install the dmsg CLI and daemon on Windows x64.
#
#   irm https://raw.githubusercontent.com/ldclabs/ic-panda/main/scripts/dmsg/install.ps1 | iex
#   & .\install.ps1 -Uninstall
#
# The binary comes from the latest stable SemVer release and is checked against its
# published SHA-256 and (after 1.6.0) its pinned Authenticode signer.
# Then `dmsg install` puts it in DMSG_INSTALL_DIR (default
# %LOCALAPPDATA%\dMsg\bin, which the user PATH gains), never over a newer one.
# Upgrade with `dmsg update`. Uninstalling stops the daemon, removes the login
# registration and the binary, and keeps the data in %USERPROFILE%\.dmsg.
#
# Environment: DMSG_VERSION, DMSG_INSTALL_DIR, DMSG_RELEASE_URL.

param([switch]$Uninstall, [string]$VerifyFile, [string]$SignerPem)

$ErrorActionPreference = 'Stop'

function Confirm-ReleaseSignature([string]$File, [string]$Pem) {
  $der = [Convert]::FromBase64String(($Pem -replace '-----BEGIN CERTIFICATE-----|-----END CERTIFICATE-----', ''))
  $expected = [Security.Cryptography.X509Certificates.X509Certificate2]::new($der)
  $signature = Get-AuthenticodeSignature -LiteralPath $File
  if ($signature.Status -ne 'Valid' -or !$signature.TimeStamperCertificate -or
      $signature.SignerCertificate.Thumbprint -ne $expected.Thumbprint) {
    throw "Expected a valid timestamped release signature on $File ($($signature.Status))"
  }
}

if ($VerifyFile) {
  Confirm-ReleaseSignature $VerifyFile (Get-Content -LiteralPath $SignerPem -Raw)
  return
}
$OfficialReleases = 'https://github.com/ldclabs/ic-panda/releases'
$Releases = if ($env:DMSG_RELEASE_URL) { $env:DMSG_RELEASE_URL } else { $OfficialReleases }
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
  $Tag = $env:DMSG_VERSION
} elseif ($Releases -eq $OfficialReleases) {
  $best = $null
  for ($page = 1; ; $page++) {
    $items = Invoke-RestMethod -Uri "https://api.github.com/repos/ldclabs/ic-panda/releases?per_page=100&page=$page"
    foreach ($item in $items) {
      if ($item.draft -or $item.prerelease -or $item.tag_name -notmatch '^\d+\.\d+\.\d+$') { continue }
      $names = @($item.assets | Where-Object { $_.state -eq 'uploaded' } | ForEach-Object { $_.name })
      if ($names -contains $Asset -and $names -contains "$Asset.sha256" -and [version]$item.tag_name -gt $best) {
        $best = [version]$item.tag_name
      }
    }
    if ($items.Count -lt 100) { break }
  }
  if (!$best) { throw 'No complete stable dMsg Windows release' }
  $Tag = $best.ToString()
} else {
  $Tag = (Invoke-RestMethod -Uri "$Releases/latest" -Headers @{ Accept = 'application/json' }).tag_name
  if ($Tag -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw "the latest release ($Tag) is not a stable dMsg client release" }
}
if ($Tag -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.(0|[1-9][0-9]*))?$') { throw 'DMSG_VERSION must be X.Y.Z or X.Y.Z-beta.N' }

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
  if ([version]($Tag -split '-')[0] -gt [version]'1.6.0') {
    $url = "https://raw.githubusercontent.com/ldclabs/ic-panda/refs/tags/$Tag/scripts/dmsg/certum-signer.pem"
    $pem = (Invoke-WebRequest -UseBasicParsing -Uri $url).Content
    if ($pem -is [byte[]]) { $pem = [Text.Encoding]::ASCII.GetString($pem) }
    Confirm-ReleaseSignature $Binary $pem
  }
  & $Binary install --dir $InstallDir
  if ($LASTEXITCODE -ne 0) { throw "dmsg install failed" }
} finally {
  Remove-Item -Recurse -Force $Work -ErrorAction SilentlyContinue
}

if ($UserPath -notcontains $InstallDir) {
  Set-UserPath ($UserPath + $InstallDir)
  Write-Host "Added $InstallDir to your user PATH; open a new terminal to use it."
}
Write-Host "Next: ``dmsg start`` and ``dmsg login`` (production; $env:USERPROFILE\.dmsg\config.toml picks another deployment)."
Write-Host "``dmsg autostart install`` starts the daemon when you log in."
