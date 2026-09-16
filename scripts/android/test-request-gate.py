#!/usr/bin/env python3
"""Exercise permission/cancellation ordering without Android or USB hardware."""
import os
from pathlib import Path
import subprocess
import tempfile

repo=Path(__file__).resolve().parents[2]
jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
with tempfile.TemporaryDirectory(prefix='opensaab-request-gate-') as output:
    subprocess.run([str(jdk/'bin/javac'),'-d',output,
        str(repo/'android/shared/com/opensaab/usb/RequestGate.java'),
        str(repo/'android/shared/com/opensaab/usb/EmulatorArchitecture.java'),
        str(repo/'android/tests/EmulatorArchitectureTest.java'),
        str(repo/'android/shared/com/opensaab/usb/CompatibilityCheck.java'),
        str(repo/'android/shared/com/opensaab/usb/InstallerChoice.java'),
        str(repo/'android/tests/CompatibilityCheckTest.java'),
        str(repo/'android/shared/com/opensaab/usb/ReleaseVersion.java'),
        str(repo/'android/tests/ReleaseVersionTest.java'),
        str(repo/'android/shared/com/opensaab/usb/DtcReport.java'),
        str(repo/'android/shared/com/opensaab/usb/VehicleIdentity.java'),
        str(repo/'android/tests/DtcReportTest.java'),
        str(repo/'android/shared/com/opensaab/usb/SsaData.java'),
        str(repo/'android/shared/com/opensaab/usb/SsaCardImport.java'),
        str(repo/'android/tests/SsaDataTest.java'),
        str(repo/'android/shared/com/opensaab/usb/SecurityApiClient.java'),
        str(repo/'android/tests/SecurityApiClientTest.java'),
        str(repo/'android/shared/com/opensaab/usb/LcdFrame.java'),
        str(repo/'android/tests/LcdFrameTest.java'),
        str(repo/'android/shared/com/opensaab/usb/ReceivePump.java'),
        str(repo/'android/tests/ReceivePumpTest.java'),
        str(repo/'android/shared/com/opensaab/usb/IgnitionStatusText.java'),
        str(repo/'android/tests/IgnitionStatusTextTest.java'),
        str(repo/'android/tests/RequestGateTest.java'),
        str(repo/'android/shared/com/opensaab/usb/NativeCommandGate.java'),
        str(repo/'android/shared/com/opensaab/usb/ChipsoftCommandGate.java'),
        str(repo/'android/tests/ChipsoftCommandGateTest.java'),
        str(repo/'android/tests/NativeCommandGateTest.java')],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.RequestGateTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.EmulatorArchitectureTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.CompatibilityCheckTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.ReleaseVersionTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.ReceivePumpTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.NativeCommandGateTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.ChipsoftCommandGateTest'],check=True)

    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.IgnitionStatusTextTest'],check=True)

    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.LcdFrameTest'],check=True)

    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.SsaDataTest'],check=True)
    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.SecurityApiClientTest'],check=True)

    subprocess.run([str(jdk/'bin/java'),'-cp',output,'com.opensaab.usb.DtcReportTest'],check=True)
