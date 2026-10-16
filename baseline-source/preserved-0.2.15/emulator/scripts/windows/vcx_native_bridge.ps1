# Build a fixed installed-driver profile as x86; preserve the other adapter EXE.
param(
 [ValidateSet('nano','chipsoft-pro')][string]$Adapter='nano',
 [string]$SourcePath=(Join-Path $PSScriptRoot 'vcx_native_bridge.cs'),
 [string]$OutputDirectory=$env:USERPROFILE
)
$ErrorActionPreference='Stop'
$name=if($Adapter -eq 'chipsoft-pro'){'chipsoft-native-bridge.exe'}else{'vcx-native-bridge.exe'}
$output=Join-Path $OutputDirectory $name
$compilerArgs=@('/nologo','/platform:x86','/target:exe',"/out:$output")
if($Adapter -eq 'chipsoft-pro'){$compilerArgs += '/define:CHIPSOFT_PRO'}
& "$env:WINDIR\Microsoft.NET\Framework\v4.0.30319\csc.exe" @compilerArgs $SourcePath
if($LASTEXITCODE -ne 0){throw "Native bridge compilation failed: $LASTEXITCODE"}
Write-Output "BUILT adapter=$Adapter path=$output"
