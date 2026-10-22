#!/usr/bin/env python3
"""Targeted lifecycle tests; no Android transport or vehicle access."""
import os, subprocess, tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
with tempfile.TemporaryDirectory(prefix='opensaab-session-safety-') as out:
    files=['android/shared/com/opensaab/usb/NativeProcessExit.java','android/shared/com/opensaab/usb/NativeSessionEnd.java','android/shared/com/opensaab/usb/FirmwareGate.java','android/adapters/chipsoft/com/opensaab/usb/ChipsoftNativeStopGate.java','android/tests/NativeProcessExitTest.java']
    subprocess.run([str(jdk/'bin/javac'),'-d',out,*[str(repo/name) for name in files]],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',out,'com.opensaab.usb.NativeProcessExitTest'],check=True)
