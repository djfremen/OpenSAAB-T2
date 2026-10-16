# SPDX-License-Identifier: MPL-2.0
param([switch]$Offline)
$ErrorActionPreference='Stop'
$root=$PSScriptRoot
$run=Join-Path $root ('runs\'+(Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $run -Force | Out-Null
$env:LOAD_TEST_ACCELERATE='1'
$argsList=@('--research-harness','--candi-native-link','--candi-on-demand','--max-insns','50000000000','--warmup-insns','1000000000','--output-dir',('"'+$run+'"'),'--boot',('"'+$root+'\runtime\eprom.bin"'),'--opsys',('"'+$root+'\runtime\opsys.dwn"'),'--candi-firmware',('"'+$root+'\runtime\candi.bin"'),('"'+$root+'\runtime\Saab NAO.bin"'))
if(!$Offline){$argsList+=@('--candi-j2534-local','--candi-j2534-adapter','nano')}
$p=Start-Process -FilePath (Join-Path $root 'tech2-emu.exe') -ArgumentList $argsList -WorkingDirectory $root -WindowStyle Normal -RedirectStandardOutput (Join-Path $run 'stdout.log') -RedirectStandardError (Join-Path $run 'stderr.log') -PassThru
@{pid=$p.Id;run=$run;adapter=$(if($Offline){'offline'}else{'nano-native-j2534'});started=(Get-Date -Format o)} | ConvertTo-Json | Set-Content (Join-Path $root 'latest-session.json')
