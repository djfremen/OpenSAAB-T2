# Emulator health reporting

The session screen always displays: **Swipe ↑ / ↓ · Hold for ENTER · EXIT goes back**. Tap this hint for the full control guide.

Local health monitoring offers **Send report to OpenSAAB** or **Keep waiting** when:

- the emulation loop stops publishing its one-second heartbeat for 30 seconds (initial startup allows 180 seconds);
- a requested firmware key has no subsequent display update for 30 seconds;
- the foreground Android UI does not service its heartbeat for 20 seconds (the dialog can only appear after the UI responds again);
- an active emulator session ends without an expected stop.

These are suspected problems, not proof of a crash. A static display with a healthy loop and no pending input does not trigger a warning. A key pressed at a menu boundary or a long vehicle operation can still trigger a suspected-response warning; Keep waiting leaves the session alone. At most one warning is shown per session.

Detection does not send vehicle commands, reset security data, restart firmware, or upload anything. Sending a report stops the session, opens the report form, and requires review and explicit Send. Reports include a bounded health reason and timing counters; raw bus data, VINs, security responses, and screenshots remain excluded. Uncaught Java crashes and unshown health incidents offer reporting on next launch. A full process kill without a saved incident may not be detected.

Shared monitoring covers offline emulation and native Chipsoft/Nano sessions, on both APK architectures. Startup timing is deliberately tolerant of low-memory head units. Expected user stops do not overwrite the last error record with a shutdown exception.
