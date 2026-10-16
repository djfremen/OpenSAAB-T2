# SPDX-License-Identifier: MPL-2.0
param(
 [Parameter(Mandatory=$true)][string]$RuntimeDirectory,
 [string]$Destination=(Join-Path $env:LOCALAPPDATA 'OpenSAAB\WindowsPreview'),
 [switch]$SkipBuild
)
$ErrorActionPreference='Stop'
$source=(Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if(!$SkipBuild){
 & "$env:USERPROFILE\.cargo\bin\cargo.exe" build --manifest-path "$source\Cargo.toml" --release --locked --features load-test --bin tech2-emu
 if($LASTEXITCODE -ne 0){throw 'Emulator build failed'}
}
foreach($name in @('eprom.bin','opsys.dwn','candi.bin','Saab NAO.bin')){
 if(!(Test-Path (Join-Path $RuntimeDirectory $name))){throw "Missing runtime image: $name"}
}
New-Item -ItemType Directory -Force $Destination,(Join-Path $Destination 'runtime') | Out-Null
Copy-Item "$source\target\release\tech2-emu.exe" $Destination
& "$PSScriptRoot\vcx_native_bridge.ps1" -Adapter nano -OutputDirectory $Destination
foreach($name in @('eprom.bin','opsys.dwn','candi.bin','Saab NAO.bin')){Copy-Item (Join-Path $RuntimeDirectory $name) (Join-Path $Destination 'runtime')}
Copy-Item "$PSScriptRoot\Start-OpenSAAB.ps1" $Destination
Copy-Item "$source\LICENSE","$source\THIRD_PARTY_NOTICES.md" $Destination
$w=New-Object -ComObject WScript.Shell
$link=$w.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Desktop')) 'OpenSAAB Windows - Nano.lnk'))
$link.TargetPath="$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe"
$link.Arguments='-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "'+(Join-Path $Destination 'Start-OpenSAAB.ps1')+'"'
$link.WorkingDirectory=$Destination
$link.Description='OpenSAAB Windows preview - native VCX Nano driver'
$link.Save()
Get-FileHash "$Destination\tech2-emu.exe","$Destination\vcx-native-bridge.exe" | Select-Object Path,Hash
