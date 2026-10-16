# Windows parent watchdog. The child uses the 32-bit OEM DLL.
# Run through the normal PowerShell invocation allowed by the host's policy.
param(
    [string]$ProbePath = "$env:USERPROFILE\vcx_t8_vin.ps1",
    [ValidateRange(1,60)][int]$TimeoutSeconds = 25,
    [string]$LogDirectory = "$env:USERPROFILE\vcx-bench-logs",
    [ValidateSet("Vin","Identity")][string]$Profile = "Vin"
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$child = $null
$exitCode = 1
$reader = $null
$lock = $null
try {
    # Cross-process guard: do not overlap host reads on the same adapter.
    $lockPath = Join-Path $env:TEMP 'tech2-vcx-read.lock'
    $lock = [IO.File]::Open($lockPath, 'OpenOrCreate', 'ReadWrite', 'None')
    $source = "`$env:VCX_READ_PROFILE='$Profile';`n" + (Get-Content -LiteralPath $ProbePath -Raw)
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($source))
    $null = New-Item -ItemType Directory -Force -Path $LogDirectory
    $stamp = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N')
    $stdout = Join-Path $LogDirectory "$stamp.out.log"
    $stderr = Join-Path $LogDirectory "$stamp.err.log"
    $powershell = "$env:WINDIR\SysWOW64\WindowsPowerShell\v1.0\powershell.exe"
    $child = Start-Process $powershell -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-EncodedCommand',$encoded) -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $handle = $child.Handle
    Write-Output "HELPER_PID=$($child.Id) WATCHDOG_SECONDS=$TimeoutSeconds"
    Write-Output "STDOUT_LOG=$stdout"
    Write-Output "STDERR_LOG=$stderr"
    $reader = [IO.StreamReader]::new([IO.File]::Open($stdout, 'Open', 'Read', 'ReadWrite'))
    $watch = [Diagnostics.Stopwatch]::StartNew()
    while (-not $child.WaitForExit(100)) {
        [Console]::Out.Write($reader.ReadToEnd())
        [Console]::Out.Flush()
        if ($watch.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
            Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
            $child.WaitForExit()
            $exitCode = 124
            Write-Output 'WATCHDOG_TIMEOUT: helper terminated; native cleanup not guaranteed; inspect the adapter before retrying.'
            break
        }
    }
    if ($exitCode -ne 124) {
        $child.WaitForExit() # Finish any asynchronous redirected-output handlers.
        $exitCode = $child.ExitCode
    }
    [Console]::Out.Write($reader.ReadToEnd())
    [Console]::Out.Flush()
    Get-Content -LiteralPath $stderr
} catch {
    Write-Output "WATCHDOG_ERROR: $($_.Exception.Message)"
} finally {
    if ($null -ne $reader) { $reader.Dispose() }
    if ($null -ne $child) {
        if (-not $child.HasExited) {
            Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
            $child.WaitForExit()
        }
        $child.Dispose()
    }
    if ($null -ne $lock) { $lock.Dispose() }
}
Write-Output "HELPER_EXIT=$exitCode"
exit $exitCode
