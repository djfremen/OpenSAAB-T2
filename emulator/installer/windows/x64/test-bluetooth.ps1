# SPDX-License-Identifier: MPL-2.0
param([Parameter(Mandatory=$true)][string]$AppDirectory,[Parameter(Mandatory=$true)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
$app=[IO.Path]::GetFullPath($AppDirectory)
$output=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $output | Out-Null
$radio=@(Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Select-Object Status,Class,FriendlyName,InstanceId)
@{os=(Get-CimInstance Win32_OperatingSystem).Caption;os_version=[Environment]::OSVersion.Version.ToString();process_architecture=$env:PROCESSOR_ARCHITECTURE;native_architecture=$env:PROCESSOR_ARCHITEW6432;app_architecture='x64';bluetooth_devices=$radio;physical_radio_qualified=$false} | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $output 'windows-environment.json')
$manifest=Get-Content -LiteralPath (Join-Path $app 'payload-manifest.json') -Raw | ConvertFrom-Json
foreach($file in $manifest){$path=Join-Path $app $file.file;if(!(Test-Path -LiteralPath $path) -or (Get-FileHash -LiteralPath $path).Hash.ToLower() -ne $file.sha256.ToLower()){throw "Payload mismatch: $($file.file)"}}
$run=Start-Process -FilePath (Join-Path $app 'OpenSAAB.exe') -ArgumentList '--self-test' -PassThru
if(!$run.WaitForExit(60000)){throw 'Windows self-test needs attention; inspect the app error dialog. Do not infer a pass.'}
if($run.ExitCode -ne 0){throw 'Windows self-test failed'}
foreach($name in @('shell-selftest-result.txt','bluetooth-selftest-result.txt')){Copy-Item -LiteralPath (Join-Path $app $name) -Destination $output;Get-Content -LiteralPath (Join-Path $output $name)}
Write-Host 'Synthetic installed-file checks completed. Use the visible adapter picker for native discovery/cancel and explicit live reads. VM results do not qualify native Windows x64 hardware.'
