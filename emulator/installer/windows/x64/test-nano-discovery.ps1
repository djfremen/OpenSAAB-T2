# Discovery regression and optional adapter-only qualification. Never queries an ECU.
param(
 [Parameter(Mandatory=$true)][string]$PayloadDirectory,
 [switch]$LiveIdentity
)
$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath($PayloadDirectory)
$assembly=[Reflection.Assembly]::LoadFile((Join-Path $root 'OpenSAAB.exe'))
$profile=$assembly.GetType('AdapterProfile',$true)
$probe=$assembly.GetType('AdapterProbe',$true)
$mode=$assembly.GetType('WindowsTransport',$true)

# Exercise the actual worker's identity predicate without loading a vendor DLL.
$script=Get-Content -LiteralPath (Join-Path $root 'adapter-tools\vcx_t8_vin.ps1') -Raw
$match=[regex]::Match($script,"(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@")
if(!$match.Success){throw 'Could not locate worker C# source'}
Add-Type -TypeDefinition $match.Groups[1].Value
$checks=0
foreach($fw in @('1.9.4.2 VCX-NANO','vcx-nano'," VCX-NANO`t")){
 if(![VcxT8Vin]::IsNanoFirmware($fw)){throw 'Valid identity rejected'}
 $checks++
}
foreach($fw in @($null,'','CH343','VCX-NANO-OTHER','NOTVCX-NANO','VCX NANO')){
 if([VcxT8Vin]::IsNanoFirmware($fw)){throw 'Unverified identity accepted'}
 $checks++
}
$adapters=@($profile.GetMethod('Detect').Invoke($null,@()))
$candidates=@($adapters | Where-Object {$_.Id -eq 'nano'})
$result=[ordered]@{firmware_identity_cases=$checks;candidate_count=$candidates.Count;adapter_identity_verified=$false;ecu_requests_sent=0;cleanup_verified=$false}
if($LiveIdentity){
 if($candidates.Count -ne 1){throw 'Exactly one Nano candidate is required'}
 $state=Join-Path $root 'identity-test-state'
 $vendor=[Enum]::Parse($mode,'VendorJ2534')
 $arguments=[object[]]@([string]$root,[string]$state,$candidates[0].PSObject.BaseObject,[bool]$false,$vendor,[bool]$true)
 $task=$probe.GetMethod('Run').Invoke($null,$arguments)
 $outcome=$task.GetAwaiter().GetResult()
 if(!$outcome.NanoIdentityConfirmed){throw 'Identity or native cleanup unconfirmed; inspect private adapter logs before another attempt'}
 if($outcome.Text.Contains('PassThruWriteMsgs') -or $outcome.Text.Contains('CALL PassThruConnect')){throw 'Unexpected diagnostic activity'}
 $result.adapter_identity_verified=$true
 $result.cleanup_verified=$true
}
$result | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $root 'nano-discovery-result.json')
$result | ConvertTo-Json
