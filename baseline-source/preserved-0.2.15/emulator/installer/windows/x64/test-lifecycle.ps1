# SPDX-License-Identifier: MPL-2.0
# Destructive only to this preview package. Leaves it reinstalled on success.
param(
 [Parameter(Mandatory=$true)][string]$MsiPath,
 [Parameter(Mandatory=$true)][string]$ManifestPath,
 [Parameter(Mandatory=$true)][string]$EvidenceDirectory
)
$ErrorActionPreference='Stop'
$msi=(Resolve-Path -LiteralPath $MsiPath).Path
$manifest=Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
$installed=Join-Path $env:LOCALAPPDATA 'OpenSAAB\WindowsApp'
$data=Join-Path $env:LOCALAPPDATA 'OpenSAAB\WindowsData'
$desktop=Join-Path ([Environment]::GetFolderPath('Desktop')) 'OpenSAAB.lnk'
$menu=Join-Path ([Environment]::GetFolderPath('Programs')) 'OpenSAAB'
New-Item -ItemType Directory -Force $EvidenceDirectory,$data | Out-Null
$sentinel=Join-Path $data ('installer-preservation-test-'+[guid]::NewGuid().ToString('N')+'.txt')
'Preserve user diagnostics during uninstall.' | Set-Content $sentinel
$events=New-Object Collections.Generic.List[object]
function Record($step,$details){
 $events.Add(@{utc=[DateTime]::UtcNow.ToString('o');step=$step;details=$details})
 $events | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 (Join-Path $EvidenceDirectory 'lifecycle.json')
 Write-Output "$step : $details"
}
function RunMsi($verb,$step){
 $log=Join-Path $EvidenceDirectory "$step.log"
 $p=Start-Process msiexec.exe -ArgumentList @($verb,('"'+$msi+'"'),'/qn','/norestart','/L*v',('"'+$log+'"')) -Wait -PassThru
 Record $step "exit=$($p.ExitCode)"
 if($p.ExitCode -notin @(0,3010)){throw "$step failed: $($p.ExitCode). Inspect $log"}
}
function VerifyInstalled($step){
 foreach($file in $manifest){
  $path=Join-Path $installed $file.file
  if(!(Test-Path -LiteralPath $path)){throw "Missing installed file: $($file.file)"}
  if((Get-FileHash -LiteralPath $path).Hash -ne $file.sha256){throw "Installed hash mismatch: $($file.file)"}
 }
 foreach($path in @($desktop,(Join-Path $menu 'OpenSAAB.lnk'))){
  if(!(Test-Path -LiteralPath $path)){throw "Missing shortcut: $path"}
  $link=(New-Object -ComObject WScript.Shell).CreateShortcut($path)
  if($link.TargetPath -ne (Join-Path $installed 'OpenSAAB.exe')){throw "Wrong shortcut target: $path"}
 }
 $entry=Get-ItemProperty @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*') -ErrorAction SilentlyContinue | Where-Object {$_.DisplayName -eq 'OpenSAAB Windows Preview'}
 if(@($entry).Count -ne 1 -or $entry.DisplayVersion -ne '0.2.7'){throw 'Incorrect installed-program registration'}
 Record $step "All $($manifest.Count) file hashes, two shortcuts, and version 0.2.7 registration verified."
}
try {
 if(Get-Process tech2-emu,OpenSAAB,vcx-native-bridge,chipsoft-native-bridge -ErrorAction SilentlyContinue){throw 'Close OpenSAAB and adapter sessions before testing installation.'}
 RunMsi '/i' 'install'
 VerifyInstalled 'installed-verification'
 Remove-Item -LiteralPath (Join-Path $installed 'OpenSAAB.exe')
 RunMsi '/fa' 'repair'
 VerifyInstalled 'repair-verification'
 RunMsi '/x' 'uninstall'
 foreach($file in $manifest){if(Test-Path -LiteralPath (Join-Path $installed $file.file)){throw "Uninstall left packaged file: $($file.file)"}}
 if((Test-Path $desktop) -or (Test-Path $menu)){throw 'Uninstall left shortcuts'}
 $entry=Get-ItemProperty @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*') -ErrorAction SilentlyContinue | Where-Object {$_.DisplayName -eq 'OpenSAAB Windows Preview'}
 if($entry){throw 'Uninstall left installed-program registration'}
 if(!(Test-Path -LiteralPath $sentinel)){throw 'Uninstall removed user diagnostics'}
 Record 'uninstall-verification' 'Package files, shortcuts and registration removed; user data retained.'
 RunMsi '/i' 'reinstall'
 VerifyInstalled 'reinstall-verification'
 Remove-Item -LiteralPath $sentinel
 Record 'complete' 'MSI lifecycle passed. GUI launch and live adapter checks are separate.'
} catch {
 Record 'failed' $_.Exception.Message
 throw
}
