# Builds the release .exe and the installer into target\installer. Needs Inno Setup 6.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root
# Start clean so old versions do not stay next to the new installer.
Remove-Item target\installer -Recurse -Force -ErrorAction SilentlyContinue

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
cargo build --release --locked
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) was not found.' }

# build.rs writes the icon into its output folder; use the newest one.
$icon = Get-ChildItem target\release\build\rdp-manager-*\out\app.ico |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
& $iscc /Q "/DAppVersion=$version" "/DAppIcon=$($icon.FullName)" installer\rdp-manager.iss
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$out = 'target\installer'
Copy-Item target\release\rdp-manager.exe $out
foreach ($file in "rdp-manager.exe", "rdp-manager-$version-setup.exe") {
    $hash = (Get-FileHash "$out\$file" -Algorithm SHA256).Hash.ToLower()
    Set-Content "$out\$file.sha256" "$hash  $file" -NoNewline
}
Get-ChildItem $out
