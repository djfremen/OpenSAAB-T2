# Pure command-gate checks. Loads the x86 assembly but never calls its driver/Run.
param([string]$AssemblyPath=(Join-Path $env:USERPROFILE 'vcx-native-bridge.exe'))
$ErrorActionPreference='Stop'
if ([IntPtr]::Size -ne 4) { throw 'Run these checks in 32-bit PowerShell' }
[void][Reflection.Assembly]::LoadFile($AssemblyPath)
$checks=New-Object 'System.Collections.Generic.List[bool]'
function Check-Gate([uint32]$controller,[uint32]$flags,[uint32]$id,[byte[]]$data,[bool]$expected) {
    $actual=[VcxNativeBridge]::Allowed($controller,$flags,$id,$data)
    if ($actual -ne $expected) { throw "Gate mismatch controller=$controller flags=$flags id=$id data=$data" }
    $checks.Add($true)
}
Check-Gate 2 1024 256 ([byte[]]@()) $true
Check-Gate 0 1024 256 ([byte[]]@()) $false
Check-Gate 2 1024 257 ([byte[]]@()) $false
Check-Gate 2 0 577 ([byte[]]@(2,26,144,0,0,0,0,0)) $true
Check-Gate 2 0 577 ([byte[]]@(3,170,1,1,0,0,0,0)) $true
Check-Gate 2 0 577 ([byte[]]@(48,0,0,0,0,0,0,0)) $true
Check-Gate 0 0 257 ([byte[]]@(254,1,62,0,0,0,0,0)) $true
Check-Gate 0 0 2016 ([byte[]]@(3,169,129,18,0,0,0,0)) $true
Check-Gate 0 0 2017 ([byte[]]@(3,169,129,16,0,0,0,0)) $true
Check-Gate 0 0 2017 ([byte[]]@(2,26,144,0,0,0,0,0)) $true
Check-Gate 2 0 2017 ([byte[]]@(3,169,129,16,0,0,0,0)) $false
Check-Gate 0 0 2017 ([byte[]]@(1,4,0,0,0,0,0,0)) $false
Check-Gate 0 0 2017 ([byte[]]@(2,39,1,0,0,0,0,0)) $false
Check-Gate 0 0 2017 ([byte[]]@(4,39,2,18,52,0,0,0)) $false
Check-Gate 0 0 2017 ([byte[]]@(2,52,0,0,0,0,0,0)) $false
Check-Gate 0 0 2017 ([byte[]]@(3,59,1,0,0,0,0,0)) $false
Check-Gate 2 0 577 ([byte[]]@(3,39,2,0,0,0,0,0)) $false
Check-Gate 2 0 577 ([byte[]]@(1,4,0,0,0,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(2,52,0,0,0,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(16,19,26,144,0,0,0,0)) $false
Check-Gate 1 0 257 ([byte[]]@(254,1,62,0,0,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(7,26,144)) $false
Check-Gate 2 0 100 ([byte[]]@(2,26,144)) $false
Check-Gate 0 0 2016 ([byte[]]@(4,44,243,0,28,0,0,0)) $true
Check-Gate 0 0 2016 ([byte[]]@(4,44,243,0,1,0,0,0)) $true
Check-Gate 2 0 577 ([byte[]]@(4,44,243,0,1,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(4,44,243,0,2,0,0,0)) $false
Check-Gate 2 0 577 ([byte[]]@(4,44,243,0,28,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(4,44,243,0,29,0,0,0)) $false
Check-Gate 0 0 2016 ([byte[]]@(3,170,1,243,0,0,0,0)) $true
Check-Gate 0 0 2016 ([byte[]]@(3,170,3,243,0,0,0,0)) $false
Check-Gate 2 0 591 ([byte[]]@(3,170,1,3,0,0,0,0)) $true
Check-Gate 2 0 591 ([byte[]]@(3,170,3,3,0,0,0,0)) $false
foreach($level in 1,11,2,12) {
    $expected=$level -eq 1 -or $level -eq 11
    $actual=[VcxNativeBridge]::AllowedForSession(2,0,577,([byte[]]@(2,39,$level,0,0,0,0,0)),$true)
    if($actual -ne $expected){throw "Seed-only level gate mismatch $level"}
    $checks.Add($true)
}
Check-Gate 2 0 577 ([byte[]]@(2,39,1,0,0,0,0,0)) $false
if([VcxNativeBridge]::AllowedForSession(2,0,578,([byte[]]@(2,39,11)),$true)){throw 'SAS seed allowed on wrong module'}
$checks.Add($true)
if([VcxNativeBridge]::AllowedForSession(2,0,577,([byte[]]@(4,39,2,18,52)),$true)){throw 'Key submission allowed'}
$checks.Add($true)
if(-not [VcxNativeBridge]::AllowedForSession(0,0,581,([byte[]]@(2,39,1)),$true)){throw 'Native HS245 level 1 seed rejected'}
$checks.Add($true)
if([VcxNativeBridge]::AllowedForSession(0,0,581,([byte[]]@(2,39,11)),$true)){throw 'SAS seed allowed on HS alias'}
$checks.Add($true)
Check-Gate 2 0 577 ([byte[]]@(2,174,0)) $false
foreach($case in @(
    @{ Controller=2; Id=577; Data=([byte[]]@(2,174,0,0,0,0,0,0)); Expected=$true },
    @{ Controller=0; Id=577; Data=([byte[]]@(2,174,0)); Expected=$false },
    @{ Controller=2; Id=578; Data=([byte[]]@(2,174,0)); Expected=$false },
    @{ Controller=2; Id=577; Data=([byte[]]@(2,174,1)); Expected=$false },
    @{ Controller=2; Id=577; Data=([byte[]]@(3,174,0,1)); Expected=$false }
)) {
    $actual=[VcxNativeBridge]::AllowedForSession($case.Controller,0,$case.Id,$case.Data,$true)
    if($actual -ne $case.Expected){throw 'Exact native SAS preamble gate mismatch'}
    $checks.Add($true)
}
foreach($case in @(
 @{ Chipsoft=$true; Rc=7; Count=0; ErrorRc=0; Detail='ERR_EMPTY_MESSAGES_QUEUE'; Expected=$true },
 @{ Chipsoft=$false; Rc=7; Count=0; ErrorRc=0; Detail='ERR_EMPTY_MESSAGES_QUEUE'; Expected=$false },
 @{ Chipsoft=$true; Rc=7; Count=1; ErrorRc=0; Detail='ERR_EMPTY_MESSAGES_QUEUE'; Expected=$false },
 @{ Chipsoft=$true; Rc=7; Count=0; ErrorRc=0; Detail='ERR_DEVICE_DISCONNECTED'; Expected=$false },
 @{ Chipsoft=$true; Rc=7; Count=0; ErrorRc=1; Detail='ERR_EMPTY_MESSAGES_QUEUE'; Expected=$false },
 @{ Chipsoft=$true; Rc=8; Count=0; ErrorRc=0; Detail='ERR_EMPTY_MESSAGES_QUEUE'; Expected=$false }
)) {
 $actual=[VcxNativeBridge]::IsChipsoftEmptyRead($case.Chipsoft,$case.Rc,$case.Count,$case.ErrorRc,$case.Detail)
 if($actual -ne $case.Expected){throw 'Chipsoft empty-read classification mismatch'}
 $checks.Add($true)
}
# Seed collection must not admit clear, programming, keys or arbitrary levels.
foreach($data in @(
 ([byte[]]@(1,4)),
 ([byte[]]@(4,20,255,255,255)),
 ([byte[]]@(2,52,0)),
 ([byte[]]@(2,54,0)),
 ([byte[]]@(4,39,12,18,52)),
 ([byte[]]@(2,39,3))
)) {
 if([VcxNativeBridge]::AllowedForSession(2,0,577,$data,$true)){throw 'Seed session admitted a non-collection command'}
 $checks.Add($true)
}
Write-Output "PURE_GATE_TESTS=$($checks.Count) PASS; hardware_opened=false"
