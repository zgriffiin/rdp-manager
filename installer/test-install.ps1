# Installs the given setup .exe silently for the current user, checks the result,
# then uninstalls it and checks that the files and shortcut are gone.
param([Parameter(Mandatory)][string]$Setup)
$ErrorActionPreference = 'Stop'

$app = "$env:LOCALAPPDATA\Programs\RDP Manager"
$shortcut = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\RDP Manager.lnk"
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{4AD11AE1-B008-4D4A-AEA9-89E08D181B5D}_is1'
if (Test-Path $uninstallKey) {
    throw 'RDP Manager is already installed for this account. Uninstall it before you run this test.'
}
function Check($ok, $what) {
    if (-not $ok) { throw "FAIL: $what" }
    Write-Output "ok: $what"
}

$p = Start-Process $Setup -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CURRENTUSER' -Wait -PassThru
Check ($p.ExitCode -eq 0) "setup exit code $($p.ExitCode)"
Check (Test-Path "$app\rdp-manager.exe") "app installed to $app"
$target = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut).TargetPath
Check ($target -eq "$app\rdp-manager.exe") "Start menu shortcut points to $target"
$entry = Get-ItemProperty $uninstallKey
Check ($entry.DisplayName -eq 'RDP Manager') "listed in Installed apps as '$($entry.DisplayName)' $($entry.DisplayVersion)"

$p = Start-Process "$app\unins000.exe" -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
Check ($p.ExitCode -eq 0) "uninstall exit code $($p.ExitCode)"
# The uninstaller finishes in a copy of itself, so wait for the folder to go.
for ($i = 0; $i -lt 60 -and (Test-Path $app); $i++) { Start-Sleep 1 }
Check (-not (Test-Path $app)) 'app folder removed'
Check (-not (Test-Path $shortcut)) 'Start menu shortcut removed'
Check (-not (Test-Path $uninstallKey)) 'Installed apps entry removed'
