# Intel Nano functional tester-present experiment

Local candidate **0.2.26-intel-preview.5** extends `.4` with a separately
identified, bounded startup experiment. It is not a qualified ME9.6 fix or a
public release.

After an owner-confirmed ignition cycle, the exact `.4` app received service-1A
NRC78, correctly waited under its four-second deadline, and received no complete
VIN. Startup, channel shutdown and USB close passed. This exercises the pending
correction live but leaves the missing positive response unresolved.

Historical owner Tech2Win captures contain successful ME9.6 VIN reads in
54–134 ms. In `cstech2win_1778872505412.log`, the functional `101: FE 3E`
D-PDU request precedes `7E0: 1A 90` on the same logical link `0xB`. The established
native classical-CAN read profile encodes that functional tester-present as
`101: FE 01 3E 00 00 00 00 00`. Presence in a successful capture does not prove
it is required or sufficient for Nano startup.

The new explicit shared probe option `--hs-vin-gm-check` sends that one functional
tester-present on HS-CAN, receives for a bounded 100 ms, and sends one unchanged
VIN request. It retains the four-second absolute reply deadline and sends one
flow-control frame only after a valid VIN first frame. The legacy
`--hs-vin-check` remains unchanged. Both use the existing native Nano read gate,
electrical-route validation and frame encoder. No SW-CAN wake, ECU write, code
clear, security request, adapter reboot or automatic retry is added.

This Intel candidate deliberately selects the experimental option. Its binding
requires the exact profile marker, one tester-present, three total CAN writes
on successful VIN assembly, verified fresh startup and acknowledged cleanup.
Failed reads retain a bounded private helper console log. USB write completion
does not establish bus acknowledgment. Security/password policy is unchanged.

The implementation is shared Rust with a thin receipt-checking native binding.
Other installed platforms retain their previous mode and qualification. Exact
installed checks, physical results, artifact hashes and private historical
evidence are retained separately. Original-firmware diagnostics still require a
fresh live VIN; lookup metadata does not satisfy that gate. The contributor's
Swedish crash remains a separate unresolved investigation.
