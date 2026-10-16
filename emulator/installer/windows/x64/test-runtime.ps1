# SPDX-License-Identifier: MPL-2.0
# Runs real firmware without an adapter. Preserves artifacts in a unique test directory.
param(
 [Parameter(Mandatory=$true)][string]$PayloadDirectory,
 [switch]$OemMenus,
 [string]$SourceDirectory=$PSScriptRoot
)
$ErrorActionPreference='Stop'
$catalog=Join-Path $SourceDirectory 'firmware-catalog.json'
if(!(Test-Path $catalog)){$catalog=Join-Path $SourceDirectory '..\..\..\ui\firmware-catalog.json'}
$compiler=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $compiler /nologo /target:exe /platform:x64 /optimize+ /main:RuntimeChecks /reference:System.Windows.Forms.dll /reference:System.Security.dll /reference:System.Drawing.dll /reference:System.Management.dll /reference:System.Web.Extensions.dll /reference:System.IO.Compression.dll /reference:System.IO.Compression.FileSystem.dll "/resource:$catalog,firmware-catalog.json" "/out:$PayloadDirectory\RuntimeChecks.exe" "$SourceDirectory\RuntimeChecks.cs" "$SourceDirectory\Firmware.cs" "$SourceDirectory\Session.cs" "$SourceDirectory\Adapter.cs" "$SourceDirectory\Transport.cs" "$SourceDirectory\SessionMode.cs" "$SourceDirectory\Launcher.cs" "$SourceDirectory\ShellTests.cs" "$SourceDirectory\TransportTests.cs" "$SourceDirectory\SupportTests.cs" "$SourceDirectory\SupportReports.cs" "$SourceDirectory\SecurityStatus.cs" "$SourceDirectory\SecurityWorkflow.cs" "$SourceDirectory\SecurityWorkflowTests.cs"
if($LASTEXITCODE -ne 0){throw 'Runtime check compilation failed'}
if($OemMenus){& "$PayloadDirectory\RuntimeChecks.exe" --oem-menus}else{& "$PayloadDirectory\RuntimeChecks.exe"}
if($LASTEXITCODE -ne 0){throw 'Runtime checks failed; preserve output and session logs'}
