param(
    [string]$Root = "$env:USERPROFILE\OpenSAAB\WindowsHarness",
    [string]$Python = "$env:LOCALAPPDATA\Programs\Python\Python313-32\python.exe"
)
$ErrorActionPreference='Stop'
if (!(Test-Path $Python)) { throw "Python not found: $Python" }
& $Python -c 'from PIL import ImageGrab; import ctypes'
if ($LASTEXITCODE -ne 0) { throw 'Python needs Pillow' }
New-Item -ItemType Directory -Force $Root | Out-Null
$identity=[Security.Principal.WindowsIdentity]::GetCurrent().Name
# Keep credentials and diagnostic captures private to the operator and SYSTEM.
& icacls.exe $Root /inheritance:r /grant:r "${identity}:(OI)(CI)F" 'SYSTEM:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Failed to restrict harness directory ACL' }
# Ask an existing supervised agent to exit normally, then stop the task.
try {
    $oldToken=(Get-Content "$Root\token.txt" -Raw).Trim()
    Invoke-RestMethod -Uri 'http://127.0.0.1:18765/shutdown' -Method Post -Headers @{Authorization="Bearer $oldToken"} -Body '{}' -ContentType 'application/json' -TimeoutSec 3 | Out-Null
    Start-Sleep -Seconds 1
} catch { }
Stop-ScheduledTask -TaskName 'OpenSAAB-Windows-Harness' -ErrorAction SilentlyContinue
# Older unsupervised agents may not implement /shutdown. Restrict fallback to
# the exact agent script path installed in this runtime directory.
Get-CimInstance Win32_Process -Filter "Name='pythonw.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains("$Root\agent.py") } | ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 500
Copy-Item "$PSScriptRoot\agent.py" "$Root\agent.py" -Force
Copy-Item "$PSScriptRoot\supervisor.py" "$Root\supervisor.py" -Force
foreach($name in @('recipes','templates','artifacts')) { New-Item -ItemType Directory -Force "$Root\$name" | Out-Null }
if (Test-Path "$PSScriptRoot\recipes") { Copy-Item "$PSScriptRoot\recipes\*.json" "$Root\recipes" -Force }
if (!(Test-Path "$Root\config.json")) {
    $bin='C:\Program Files (x86)\General Motors\Tech2Win\bin'
    $conf=@{
        usbpcap='C:\Program Files\USBPcap\USBPcapCMD.exe'
        apps=@{
            notepad=@{exe='C:\Windows\System32\notepad.exe';args=@();title='Untitled - Notepad'}
            tech2win=@{exe="$bin\emulator.exe";cwd=$bin;args=@('-of','C:\Users\Public\General Motors\Tech2Win\config\Saab NAO.conf');title='TECH2-SCREEN';env=@{Tech2WinUserData=[Environment]::GetEnvironmentVariable('Tech2WinUserData','Machine')}}
            vxmanager=@{exe='C:\Program Files (x86)\VCX\Manager\ManagerCenter.exe';args=@();title='VX Manager'}
        }
    }
    $conf | ConvertTo-Json -Depth 8 | Set-Content "$Root\config.json" -Encoding UTF8
}
# Python reads UTF-8 without BOM; Windows PowerShell writes UTF-8 with BOM.
$config=Get-Content "$Root\config.json" -Raw
[IO.File]::WriteAllText("$Root\config.json",$config,(New-Object Text.UTF8Encoding($false)))
$PythonWindowless=Join-Path (Split-Path $Python) 'pythonw.exe'
if (!(Test-Path $PythonWindowless)) { throw 'pythonw.exe not found' }
$action=New-ScheduledTaskAction -Execute $PythonWindowless -Argument "`"$Root\supervisor.py`" --root `"$Root`"" -WorkingDirectory $Root
$trigger=New-ScheduledTaskTrigger -AtLogOn -User $identity
$principal=New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Highest
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName 'OpenSAAB-Windows-Harness' -Description 'Authenticated localhost desktop harness; logs in user session; no automatic vehicle tests.' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName 'OpenSAAB-Windows-Harness'
$deadline=(Get-Date).AddSeconds(15)
do {
    Start-Sleep -Milliseconds 300
    try {
        $token=(Get-Content "$Root\token.txt" -Raw).Trim()
        $health=Invoke-RestMethod -Uri 'http://127.0.0.1:18765/health' -Method Post -Headers @{Authorization="Bearer $token"} -Body '{}' -ContentType 'application/json'
        $health | ConvertTo-Json
        Write-Output "Installed $Root"
        exit 0
    } catch { }
} while ((Get-Date) -lt $deadline)
throw 'Harness did not become healthy; inspect scheduled task and events.jsonl'
