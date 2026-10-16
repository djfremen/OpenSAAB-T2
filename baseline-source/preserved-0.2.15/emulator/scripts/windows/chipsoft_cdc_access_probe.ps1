# SPDX-License-Identifier: MPL-2.0
# Standalone CDC GET_INFO only. Does not load J2534 or open a vehicle channel.
param([Parameter(Mandatory=$true)][string]$Port)
$ErrorActionPreference='Stop'
$device=Get-CimInstance Win32_PnPEntity | Where-Object {$_.DeviceID -like 'USB\VID_0483&PID_5740\*' -and $_.Name -like "*($Port)"}
if(!$device){throw 'Selected port is not the enumerated Chipsoft CDC candidate'}
$device | Format-List Name,Status,ConfigManagerErrorCode,DeviceID
Get-CimInstance Win32_PnPSignedDriver | Where-Object {$_.DeviceID -eq $device.DeviceID} | Format-List DriverProviderName,DriverVersion,InfName
$serial=New-Object IO.Ports.SerialPort $Port,115200,None,8,One
$serial.ReadTimeout=3000
$serial.WriteTimeout=3000
$serial.DtrEnable=$false
$serial.RtsEnable=$false
try {
 $serial.Open()
 $serial.DiscardInBuffer()
 $serial.DiscardOutBuffer()
 $timer=[Diagnostics.Stopwatch]::StartNew()
 [byte[]]$tx=1,0,0,0,0,0,0,0
 $serial.Write($tx,0,8)
 'TX GET_INFO only; vehicle_channel=closed'
 $bytes=New-Object 'Collections.Generic.List[byte]'
 while($timer.ElapsedMilliseconds -lt 3000){
  $serial.ReadTimeout=[Math]::Max(1,3000-[int]$timer.ElapsedMilliseconds)
  $bytes.Add([byte]$serial.ReadByte())
  if($bytes.Count -ge 8){
   $length=[int]$bytes[2]+256*[int]$bytes[3]
   if($bytes.Count -eq 8+$length){break}
  }
 }
 $data=$bytes.ToArray()
 if($data.Length -lt 8 -or $data.Length -ne 8+$length){throw 'Incomplete reply'}
 $op=[BitConverter]::ToUInt16($data,0); $status=[BitConverter]::ToUInt16($data,4); $check=[BitConverter]::ToUInt16($data,6)
 $sum=0; for($i=8;$i -lt $data.Length;$i++){if($data[$i] -lt 32 -or $data[$i] -gt 126){throw 'Non-ASCII identity'}; $sum+=$data[$i]}
 if($op -ne 1 -or $status -ne 0 -or ($sum -band 65535) -ne $check){throw 'Invalid GET_INFO frame'}
 $identity=[Text.Encoding]::ASCII.GetString($data,8,$length)
 if(!$identity.StartsWith('CHIPSOFT J2534 Pro v. ')){throw 'Unsupported identity'}
 "IDENTIFIED: $identity elapsed_ms=$($timer.ElapsedMilliseconds)"
} finally { $serial.Dispose(); 'SERIAL_CLOSED; no vehicle communication tested' }
'Installed vendor DLL: '+(Test-Path 'C:\Program Files (x86)\CHIPSOFT_J2534_Pro_Driver\j2534_interface.dll')
