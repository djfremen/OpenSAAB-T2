# Windows desktop validation harness

Imported from the existing OpenSAAB research checkout with the Windows scan-code fix verified during MSI testing. This is developer test tooling, not an installed component of OpenSAAB. Requires Python 3.10+ and Pillow on Windows.

`agent.py` runs in an unlocked interactive desktop. It verifies the target PID/HWND and foreground window before input, refuses locked/secure desktops, and serves a bearer-token-protected API on `127.0.0.1:18765` only. `supervisor.py` restarts a crashed agent with bounded backoff. Capture jobs have bounded duration. It does not implement adapter protocols or run vehicle commands automatically.

`install.ps1` creates the developer's sign-in task and private runtime directory. Inspect its app-path defaults and supply `-Root` / `-Python` for the test host. Installation/reinstallation restarts the harness; do not run it during a capture. No LAN listener or firewall exception is created.

Connect through an authorized SSH tunnel to the Windows host. Copy its generated token to an ignored private file with restrictive permissions. Never commit tokens, config, captures, templates, or screenshots. Example from `emulator/`:

```sh
python3 scripts/windows/harness/client.py /health --token-file runs/windows-harness/token.txt
python3 scripts/windows/harness/client.py /windows --token-file runs/windows-harness/token.txt
```

Targets must use freshly observed IDs from `/windows`. `/snapshot`, `/input`, `/click`, and `/close` operate on those guarded targets. `/run` executes a recipe with required before/after screen guards. `/cancel` stops a workflow. `/capture/start` needs the actual current USB bus/device; `/capture/stop` finishes the owned capture. `/shutdown` exits the supervised agent.

Keyboard actions map virtual keys to scan codes and preserve extended navigation-key identity; plain virtual-key injection did not reach minifb, and unprefixed navigation scans became keypad digits on the test host. Unicode text input remains separate. Focus/lock/cancellation protections remain in place.

```sh
python -m unittest discover -s scripts/windows/harness -p test_harness.py
```

Seven tests passed on the Windows test host, including scan-code/Unicode regression and workflow guard cases. The vehicle-menu recipe requires separately recorded private screen templates and an authorized bench test; it is not an ECU-read result.
