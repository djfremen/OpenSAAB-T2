# Contributor USB captures (Mongoose first)

This is a separate **local-only portable preview** inside OpenSAAB-Collector.
It leaves the older Chipsoft service/shim installer intact. Do not give that
installer to a Windows 8.1/Mongoose contributor: it targets .NET 8/Windows 10,
replaces Chipsoft DLLs, and its current USB path captures a whole hub at a
96-byte snap length. Those are unsuitable defaults for this task.

The portable helper targets Windows PowerShell 4 syntax/.NET Framework APIs
available on Windows 8.1. It does not install drivers, modify adapter DLLs,
register a service, transmit diagnostic commands or upload anything. **A real
Windows 8.1 + Mongoose run is still required before calling it validated.**
The manual Wireshark workflow below can be used independently today.

## First capture: use the known-working Tech2Win setup

1. Keep the contributor's working adapter driver and Tech2Win installation.
   Windows 10 is not required solely for recording. For Windows 8.1 x64,
   Wireshark **4.0.17** was the last supported release; use the official archive,
   not the current installer. The USBPcap project supplies its own signed
   Windows driver installer. Reboot if required after installing USBPcap.
2. Close Tech2Win. Plug in the adapter and inspect Wireshark's USBPcap interface
   options (gear icon). Identify the root hub by the adapter's visible device
   entry. If uncertain, compare the list with it unplugged, then reconnect it
   and refresh **before** choosing the final address.
3. Disable capture from all devices and from newly connected devices. Select
   only the adapter's current address. Enable descriptor injection. Use full
   packets (65535-byte snap length), not the old Collector's 96-byte setting.
   Do not reconnect the adapter during this capture; its address may change.
4. Start capture **before launching Tech2Win**. Confirm packets are arriving,
   then launch Tech2Win, select the contributor's usual Mongoose driver,
   connect normally and read the VIN. Record the approximate time of each step.
5. Continue to ECM information, then **read** DTCs. Record the exact menu path,
   success or error, vehicle model/year/engine and adapter/driver versions.
   A no-codes response is useful. Do not clear codes or run programming.
6. Stop capture and save locally. Check the file contains the selected adapter's
   request AND response traffic spanning startup and the menu actions. Packet
   count alone does not prove the right device or diagnostic success.
7. Send the capture and action notes privately using an agreed destination.
   Do not post raw captures in a forum or public GitHub issue: they can contain
   VIN, device serials, security traffic or accidentally captured peripheral data.

Start with a short initialization + VIN recording if the contributor is unsure.
We can review it before asking for longer ECM/DTC captures. Security access is a
separate, optional later capture of their normal authorized workflow; it is not
needed to get started and should not be triggered merely to collect more data.

## Helper status

A separate local-only PowerShell helper is in development in OpenSAAB-Collector.
It has fixture tests, but Windows 8.1 + Mongoose hardware validation is pending.
Use the manual workflow above for now. This guide does not require installing
the legacy Chipsoft Collector or changing Windows script-execution policy.

## Why keep this in Collector?

Reuse the project name, session organization and eventual analysis pipeline,
but separate capture from adapter decoding:

- `portable/`: human-started capture and local evidence, adapter agnostic.
- Future adapter profiles: Mongoose, Chipsoft and VCX separately; verify exact
  hardware IDs and driver version, never guess from a generic USB serial name.
- Analysis: correlate action notes, API traces when available, USB transfers and
  diagnostic replies. A USB trace alone may not expose higher-level API calls.
- Upload: a future review-and-send screen, explicit destination and per-bundle
  consent; no reuse of the old automatic uploader until independently audited.

A capture supplies evidence for implementing and testing a driver. It is not a
promise that one file contains every command or is enough to support an adapter.

## Primary references

- [USBPcap project and signed installer](https://desowin.org/usbpcap/)
- [USBPcap capture workflow](https://desowin.org/usbpcap/tour.html)
- [USBPcap command-line implementation](https://github.com/desowin/usbpcap/blob/master/USBPcapCMD/cmd.c)
- [Wireshark 4.0.17 release notes: last Windows 8.1 release](https://www.wireshark.org/docs/relnotes/wireshark-4.0.17.html)
- [Official Wireshark Windows archive](https://www.wireshark.org/download/win64/all-versions/)
