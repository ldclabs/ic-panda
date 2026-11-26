# Run after electron-builder, while the SimplySign certificate is connected.
$ErrorActionPreference = 'Stop'
$release = 'src/dmsg_desktop/release'
$cli = "$release/win-unpacked/resources/runtime/dmsg.exe"
foreach ($file in @($cli, "$release/win-unpacked/dMsg.exe", "$release/dMsg-win-x64.exe")) {
    & "$PSScriptRoot/install.ps1" -VerifyFile $file -SignerPem "$PSScriptRoot/certum-signer.pem"
}

New-Item -ItemType Directory signed-cli | Out-Null
$name = 'dmsg-windows-x64.exe'
Copy-Item -LiteralPath $cli -Destination "signed-cli/$name"
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath "signed-cli/$name").Hash.ToLowerInvariant()
[IO.File]::WriteAllText("signed-cli/$name.sha256", "$hash  $name`n")
