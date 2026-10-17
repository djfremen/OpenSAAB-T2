#!/usr/bin/env python3
"""Check committed upgrade/security fixtures against an exact APK on a disposable AVD.

The preservation check snapshots the current install, reinstalls the supplied APK,
then verifies every existing files/no_backup/shared_prefs byte. Metadata identifies
whether this was a version upgrade or a same-version reinstall. No source rewrite,
physical-device access, adapter operation or report upload is performed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import zipfile

from instrumentation_results import InstrumentationResults


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--expected-version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--helper-revision", type=int, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"emulator-[0-9]+", args.serial):
        parser.error("Only an explicitly selected disposable Android emulator is permitted")
    if args.helper_revision < 1:
        parser.error("Use a distinct positive helper revision for changed test executables")
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output.resolve()
    results = InstrumentationResults(output)
    repo = Path(__file__).resolve().parents[2]
    sdk = Path(os.environ.get("ANDROID_HOME", str(Path.home() / "Library/Android/sdk")))
    jdk = Path(os.environ.get("JAVA_HOME", "/Applications/Android Studio.app/Contents/jbr/Contents/Home"))
    bt = sdk / "build-tools/36.0.0"
    android_jar = sdk / "platforms/android-36/android.jar"
    adb = [str(sdk / "platform-tools/adb"), "-s", args.serial]
    build = repo / "target/android-tech2-release"
    env = dict(os.environ, JAVA_HOME=str(jdk), PATH=str(jdk / "bin") + os.pathsep + os.environ["PATH"])
    helper_version = args.expected_version + "-evidence." + str(args.helper_revision)
    helper_package = "com.opensaab.tech2.evidence" + re.sub(r"[^0-9]", "", args.expected_version) + "r" + str(args.helper_revision)
    helper_installed = False

    def run(command, **kwargs):
        return subprocess.run(list(map(str, command)), check=True, timeout=180, **kwargs)

    def instrument(suite, *extras):
        name = suite + ("-" + extras[-1] if extras else "")
        command = adb + ["shell", "am", "instrument", "-w", "-r", "-e", "expected_version", args.expected_version,
                         *extras, helper_package + "/com.opensaab.usb." + suite]
        try:
            process = subprocess.run(command, capture_output=True, text=True, timeout=180)
        except subprocess.TimeoutExpired as error:
            def text(value):
                return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
            results.record(name, text(error.stdout), text(error.stderr), None, "instrumentation timed out")
            raise
        results.record(name, process.stdout, process.stderr, process.returncode)
        print(name + ": PASS", flush=True)

    try:
        with zipfile.ZipFile(args.apk) as package:
            metadata = json.loads(package.read("assets/build.json"))
        assert metadata["version_name"] == args.expected_version and metadata["architecture"] == "arm64-v8a"
        hardware = run(adb + ["shell", "getprop", "ro.hardware"], capture_output=True, text=True).stdout.strip()
        model = run(adb + ["shell", "getprop", "ro.product.model"], capture_output=True, text=True).stdout.strip()
        assert hardware in ("ranchu", "goldfish") and model.startswith("sdk_gphone"), (model, hardware)
        active = subprocess.run(adb + ["shell", "pidof", "libtech2_emu.so", "libtech2_recovered.so"], capture_output=True, text=True, timeout=10)
        assert not active.stdout.strip(), "Stop the active diagnostic session before instrumentation"
        before_identity = run(adb + ["shell", "dumpsys", "package", "com.opensaab.tech2"], capture_output=True, text=True).stdout
        before_version = re.search(r"versionName=([^\s]+)", before_identity).group(1)
        assert re.fullmatch(r"[A-Za-z0-9.-]+", before_version)
        (output / "before-installed-identity.txt").write_text(before_identity)
        before_path = run(adb + ["shell", "pm", "path", "com.opensaab.tech2"], capture_output=True, text=True).stdout.strip().split("package:")[-1]
        before_apk = output / ("OpenSAAB-" + before_version + "-ARM64-installed-before.apk")
        run(adb + ["pull", before_path, before_apk], stdout=subprocess.DEVNULL)
        revision = run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
        assert not run(["git", "-C", repo, "status", "--porcelain"], capture_output=True, text=True).stdout.strip(), "Commit test sources before validation"
        sources = output / "sources"
        classes = output / "classes"
        sources.mkdir()
        classes.mkdir()
        names = ["UpgradePreservationInstrumentedTest", "SecurityOpenAccessInstrumentedTest"]
        source_hashes = {}
        for name in names:
            source = repo / "android/tests" / (name + ".java")
            committed = run(["git", "-C", repo, "show", revision + ":android/tests/" + name + ".java"], capture_output=True).stdout
            assert source.read_bytes() == committed
            shutil.copy2(source, sources / source.name)
            assert (sources / source.name).read_bytes() == committed
            source_hashes[source.name] = digest(source)
        classpath = [android_jar, build / "classes", *sorted((build / "report-libraries").rglob("*.jar"))]
        classpath += [Path(item["path"]) for item in json.loads((build / "report-dependencies.json").read_text()) if item["path"].endswith(".jar")]
        with (output / "compile.log").open("w") as log:
            run([jdk / "bin/javac", "-source", "8", "-target", "8", "-cp", os.pathsep.join(map(str, classpath)), "-d", classes, *sources.glob("*.java")], env=env, stdout=log, stderr=subprocess.STDOUT)
            run([bt / "d8", "--min-api", "26", "--lib", android_jar, "--output", output, *classes.rglob("*.class")], env=env, stdout=log, stderr=subprocess.STDOUT)
        manifest = output / "AndroidManifest.xml"
        manifest.write_text('<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="' + helper_package + '" android:versionName="' + helper_version + '" android:versionCode="1"><uses-sdk android:minSdkVersion="26" android:targetSdkVersion="36"/><application android:label="OpenSAAB ' + helper_version + ' ARM64 TEST"/>' + ''.join('<instrumentation android:name="com.opensaab.usb.' + name + '" android:targetPackage="com.opensaab.tech2"/>' for name in names) + '</manifest>')
        unsigned = output / "unsigned.apk"
        run([bt / "aapt2", "link", "-I", android_jar, "--manifest", manifest, "-o", unsigned], stdout=subprocess.DEVNULL)
        with zipfile.ZipFile(unsigned, "a") as package:
            package.write(output / "classes.dex", "classes.dex")
        helper = output / ("OpenSAAB-" + helper_version + "-android-arm64-v8a-test.apk")
        signing = Path.home() / ".local/share/opensaab/release-signing"
        run([bt / "apksigner", "sign", "--ks", signing / "opensaab-release.p12", "--ks-key-alias", "opensaab-release", "--ks-pass", "file:" + str(signing / "password.txt"), "--out", helper, unsigned], env=env, stdout=subprocess.DEVNULL)
        run(adb + ["install", "-r", helper], stdout=subprocess.DEVNULL)
        helper_installed = True
        helper_identity = run(adb + ["shell", "dumpsys", "package", helper_package], capture_output=True, text=True).stdout
        assert "versionName=" + helper_version in helper_identity
        (output / "helper-installed-identity.txt").write_text(helper_identity)
        provenance = {"product_version": args.expected_version, "product_apk_sha256": digest(args.apk), "product_source_revision": metadata["source_commit"],
                      "test_source_revision": revision, "test_source_sha256": source_hashes, "source_rewritten": False,
                      "before_version": before_version, "before_apk_sha256": digest(before_apk),
                      "preservation_scenario": "same_version_reinstall" if before_version == args.expected_version else "version_upgrade",
                      "helper_version": helper_version, "helper_apk_sha256": digest(helper), "serial": args.serial, "model": model, "hardware": hardware}
        (output / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
        instrument("UpgradePreservationInstrumentedTest", "-e", "mode", "snapshot")
        run(adb + ["install", "-r", args.apk], stdout=subprocess.DEVNULL)
        installed_path = run(adb + ["shell", "pm", "path", "com.opensaab.tech2"], capture_output=True, text=True).stdout.strip().split("package:")[-1]
        installed = output / ("OpenSAAB-" + args.expected_version + "-ARM64-installed-after.apk")
        run(adb + ["pull", installed_path, installed], stdout=subprocess.DEVNULL)
        assert digest(installed) == digest(args.apk)
        instrument("UpgradePreservationInstrumentedTest", "-e", "mode", "verify")
        instrument("SecurityOpenAccessInstrumentedTest")
    except Exception as error:
        if results.status != "failed":
            try:
                results.record("runner", "", str(error), None, type(error).__name__)
            except RuntimeError:
                pass
        raise
    finally:
        if helper_installed:
            cleanup = subprocess.run(adb + ["uninstall", helper_package], capture_output=True, text=True, timeout=30)
            if cleanup.returncode:
                try:
                    results.record("helper-cleanup", cleanup.stdout, cleanup.stderr, cleanup.returncode, "helper uninstall failed")
                except RuntimeError:
                    pass
        results.finish()
    if results.status != "pass":
        raise SystemExit("Evidence validation failed; inspect run-status.json")
    print("Committed evidence checks complete; exact product APK unchanged; no physical device or adapter used.")


if __name__ == "__main__":
    main()
