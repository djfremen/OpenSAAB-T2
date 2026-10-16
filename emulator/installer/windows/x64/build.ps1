# SPDX-License-Identifier: MPL-2.0
param(
 [Parameter(Mandatory=$true)][string]$PayloadDirectory,
 [Parameter(Mandatory=$true)][string]$OutputDirectory,
 [Parameter(Mandatory=$true)][string]$WixExecutable,
 [Parameter(Mandatory=$true)][string]$VcRuntimeDll,
 [string]$DotnetExecutable='dotnet',
 [string]$SourceDirectory=$PSScriptRoot,
 [string]$BridgeSource=(Join-Path $PSScriptRoot "..\..\..\scripts\windows\vcx_native_bridge.cs")
)
$ErrorActionPreference='Stop'
$payload=[IO.Path]::GetFullPath($PayloadDirectory)
$output=[IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $payload,$output | Out-Null
$catalog=Join-Path $SourceDirectory 'firmware-catalog.json'
if(!(Test-Path $catalog)){$catalog=Join-Path $SourceDirectory '..\..\..\ui\firmware-catalog.json'}
$interface=Join-Path $SourceDirectory 'interface-contract.json'
if(!(Test-Path $interface)){$interface=Join-Path $SourceDirectory '..\..\..\ui\interface-contract.json'}
$logo=Join-Path $SourceDirectory 'opensaab.png'
if(!(Test-Path $logo)){$logo=Join-Path $SourceDirectory '..\..\..\assets\branding\opensaab-t2-logo-official.png'}
$icon=Join-Path $SourceDirectory 'opensaab.ico'
if(!(Test-Path $icon)){$icon=Join-Path $SourceDirectory '..\..\..\assets\branding\opensaab-t2-logo-official.ico'}
Copy-Item -LiteralPath $icon -Destination "$payload\opensaab.ico"
$compiler=Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $DotnetExecutable restore "$SourceDirectory\OpenSAAB.csproj" --locked-mode
if($LASTEXITCODE -ne 0){throw 'Pinned Windows SDK restore failed'}
& $DotnetExecutable build "$SourceDirectory\OpenSAAB.csproj" --no-restore -c Release -o $payload "-p:ApplicationIcon=$icon" "-p:LogoPath=$logo" "-p:CatalogPath=$catalog" "-p:InterfacePath=$interface"
if($LASTEXITCODE -ne 0){throw 'Launcher compilation failed'}
# The vendor DLL must be loaded out-of-process by an explicitly x86 helper.
if(!(Test-Path -LiteralPath $BridgeSource)){throw 'Missing J2534 bridge source'}
& $compiler /nologo /target:exe /platform:x86 /optimize+ "/out:$payload\vcx-native-bridge.exe" $BridgeSource
if($LASTEXITCODE -ne 0){throw 'Nano x86 helper compilation failed'}
& $compiler /nologo /target:exe /platform:x86 /optimize+ /define:CHIPSOFT_PRO "/out:$payload\chipsoft-native-bridge.exe" $BridgeSource
if($LASTEXITCODE -ne 0){throw 'Chipsoft x86 helper compilation failed'}
Copy-Item -LiteralPath $VcRuntimeDll -Destination "$payload\vcruntime140.dll"
$files=@(
 @('Launcher','OpenSAAB.exe','INSTALLDIR'), @('LauncherConfig','OpenSAAB.exe.config','INSTALLDIR'), @('Connection','opensaab-connection.exe','INSTALLDIR'), @('Emulator','tech2-emu.exe','INSTALLDIR'), @('Reports','opensaab-reports.exe','INSTALLDIR'), @('Security','opensaab-security.exe','INSTALLDIR'), @('ChipsoftProbe','chipsoft-probe.exe','INSTALLDIR'),
 @('NanoBridge','vcx-native-bridge.exe','INSTALLDIR'), @('ChipsoftBridge','chipsoft-native-bridge.exe','INSTALLDIR'), @('ChipsoftVin','chipsoft-vin.exe','INSTALLDIR'),
 @('ProbeWatchdog','adapter-tools\invoke_vcx_t8_vin.ps1','AdapterDirectory'), @('ChipsoftIdentity','adapter-tools\read_t8_identity.ps1','AdapterDirectory'),
 @('ChipsoftDtc','adapter-tools\read_t8_dtc.ps1','AdapterDirectory'), @('NanoIdentity','adapter-tools\vcx_t8_vin.ps1','AdapterDirectory'), @('VcRuntime','vcruntime140.dll','INSTALLDIR'),
 @('License','LICENSE','INSTALLDIR'), @('Notices','THIRD_PARTY_NOTICES.md','INSTALLDIR'),
 @('Eprom','runtime\eprom.bin','RuntimeDirectory'), @('Opsys','runtime\opsys.dwn','RuntimeDirectory'),
 @('Candi','runtime\candi.bin','RuntimeDirectory'), @('Saab','runtime\Saab NAO.bin','RuntimeDirectory')
)
$xml=New-Object Text.StringBuilder
[void]$xml.AppendLine('<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs"><Fragment><ComponentGroup Id="Payload">')
$manifest=@()
$componentGuids=@{
 Connection='4D5DC00B-5F75-4F94-A555-CF7AE2E6D82B'
 LauncherConfig='D47D1B2D-0D0B-45DD-B8D7-C7E4B519BBF1'
 Security='748CA259-9734-4D77-9FA3-3E66727495B5'
 ChipsoftProbe='C5E09D49-653B-48BB-A988-CA5A9FBD8C63'
 Reports='BD4C2ACB-DCA3-4B5B-949B-F3C67971941A'
 ChipsoftVin='C5966432-BC71-4AD6-99B8-AF19AB54B0F2'
 NanoIdentity='17DCEF51-1E84-5DCD-BA59-4AA229B00265'
 ChipsoftDtc='82EF6DD5-8C44-5C63-9A77-47AD38B4577B'
 ChipsoftIdentity='A83B182B-936C-5C52-94F8-C396FB2D675D'
 ProbeWatchdog='0B1DF3E0-6392-55F9-ACA0-5B0801CD8135'
 ChipsoftBridge='626744E4-7BD5-5D31-9287-913BC895D9AD'
 Launcher='F1E63194-E1A5-5929-8D5D-C3F6B68F9A8E'
 Emulator='8801DD1B-9A8B-587B-ADB3-3CA25BAB5923'
 NanoBridge='B6D45344-9AEA-5117-A79B-33FC779454AE'
 VcRuntime='8185B060-491D-55B1-AF53-BF1E9EC74D95'
 License='3FBFBC65-6EF7-5FF0-B5C7-B331B135E90E'
 Notices='FAFBDBD4-1487-5DC8-914B-40E2A378F70C'
 Eprom='E6C926DD-41BD-53EA-8FF0-81C24C47D293'
 Opsys='E832986E-2546-51E4-88B4-465AFC498F00'
 Candi='63970F7B-7875-5A10-9044-9122CB3C3EAA'
 Saab='47F87097-C6CB-551F-B4CD-908B5C9EC90F'
}
foreach($file in $files){
 $id,$relative,$directory=$file
 $path=Join-Path $payload $relative
 if(!(Test-Path -LiteralPath $path)){throw "Missing payload: $relative"}
 $escaped=[Security.SecurityElement]::Escape($path)
 [void]$xml.AppendLine("<Component Id=`"$id`" Directory=`"$directory`" Guid=`"$($componentGuids[$id])`"><File Id=`"$($id)File`" Source=`"$escaped`" KeyPath=`"no`"/><RegistryValue Root=`"HKCU`" Key=`"Software\OpenSAAB\WindowsPreview\Installer`" Name=`"$id`" Type=`"integer`" Value=`"1`" KeyPath=`"yes`"/></Component>")
 $manifest+=@{file=$relative;sha256=(Get-FileHash -LiteralPath $path).Hash}
}
[void]$xml.AppendLine('</ComponentGroup></Fragment></Wix>')
$xml.ToString() | Set-Content -Encoding UTF8 "$output\Payload.wxs"
$license=Get-Content -LiteralPath "$payload\LICENSE" -Raw
$rtf=$license.Replace('\','\\').Replace('{','\{').Replace('}','\}').Replace("`r",'').Replace("`n",'\par ')
('{\rtf1\ansi\deff0{\fonttbl{\f0 Arial;}}\f0\fs18 '+$rtf+'}') | Set-Content -Encoding ASCII "$payload\license.rtf"
& $WixExecutable build -arch x64 -ext WixToolset.UI.wixext -d "PayloadDir=$payload" -out "$output\OpenSAAB-0.2.17-x64.msi" "$SourceDirectory\Product.wxs" "$output\Payload.wxs"
if($LASTEXITCODE -ne 0){throw 'MSI build failed'}
$manifest | ConvertTo-Json | Set-Content -Encoding UTF8 "$output\payload-manifest.json"
Get-FileHash "$output\OpenSAAB-0.2.17-x64.msi"

@{version='0.2.17';interface_contract_sha256=(Get-FileHash -LiteralPath $interface -Algorithm SHA256).Hash.ToLower();firmware_catalog_sha256=(Get-FileHash -LiteralPath $catalog -Algorithm SHA256).Hash.ToLower();bluetooth_workflow='0.1.3';connection_worker_sha256=(Get-FileHash -LiteralPath "$payload\opensaab-connection.exe").Hash.ToLower()} | ConvertTo-Json | Set-Content -Encoding UTF8 "$output\interface-provenance.json"
