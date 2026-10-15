#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Build the shared macOS app as ARM64 or Intel, with the Android support-file profile."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
SOURCES = HERE / "shared"
EMULATOR = HERE.parents[1]
ROOT = EMULATOR.parent
VERSION = "0.2.26-intel-preview.7"
TARGETS = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}

def run(*args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)

def build_id(source_commit=None):
    """Source revision shown in support reports so QC can match a report to its exact build."""
    try:
        rev = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "--short=12", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
        dirty = subprocess.run(["git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no"], capture_output=True, text=True, check=True).stdout.strip()
        return rev + ("-dirty" if dirty else "")
    except (OSError, subprocess.CalledProcessError):
        return source_commit or "private-frozen-source"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    global VERSION
    parser.add_argument("--version", default=VERSION)
    parser.add_argument("--arch", choices=TARGETS, default="arm64")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--support-root", type=Path, default=Path(os.environ.get("OPENSAAB_SUPPORT_ROOT", str(EMULATOR))),
                        help="Root containing the approved Android support files (or three flat files)")
    parser.add_argument("--without-support", action="store_true", help="Explicit import-only package; not the standard setup profile")
    parser.add_argument("--source-commit", help="Pinned predecessor revision for the private non-Git source snapshot")
    options = parser.parse_args()
    VERSION = options.version
    support = json.loads((EMULATOR / "ui/support-files.json").read_text())
    support_inputs = {}
    if not options.without_support:
        for name, spec in support.items():
            source = options.support_root / spec["source"]
            if not source.is_file(): source = options.support_root / name
            if not source.is_file() or source.stat().st_size != spec["bytes"] or digest(source) != spec["sha256"]:
                raise SystemExit(f"Missing or invalid approved support file: {name}. Set --support-root /path/to/runtime (or explicitly build --without-support).")
            support_inputs[name] = source
    architecture = options.arch
    target_triple = TARGETS[architecture]
    output = (options.output or EMULATOR / "dist/macos" / architecture).resolve()
    output.mkdir(parents=True, exist_ok=True)
    artifact_name = f"OpenSAAB-{VERSION}-macos-{architecture}"
    destination = output / (artifact_name + ".dmg")
    if destination.exists():
        raise SystemExit(f"Output already exists; choose another --output directory: {destination}")
    rust_env = {**os.environ, "MACOSX_DEPLOYMENT_TARGET": "12.0"}
    rust_target_dir = Path(rust_env.get("CARGO_TARGET_DIR", str(EMULATOR / "target"))).resolve()
    rust_env["CARGO_TARGET_DIR"] = str(rust_target_dir)
    flags = (rust_env.get("CARGO_ENCODED_RUSTFLAGS", "").split("\x1f")
             if rust_env.get("CARGO_ENCODED_RUSTFLAGS") else rust_env.get("RUSTFLAGS", "").split())
    flags.extend([f"--remap-path-prefix={ROOT}=/opensaab",
                  f"--remap-path-prefix={Path(os.environ.get('CARGO_HOME', str(Path.home() / '.cargo')))}=/cargo"])
    rust_env.pop("RUSTFLAGS", None)
    rust_env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    run("cargo", "build", "--locked", "--release", "--features", "load-test", "--target", target_triple,
        "--manifest-path", EMULATOR / "Cargo.toml", env=rust_env)
    with tempfile.TemporaryDirectory(prefix="opensaab-macos-") as temp:
        stage = Path(temp)
        disk = stage / "disk"
        app = disk / (artifact_name + ".app")
        contents = app / "Contents"
        binaries = contents / "MacOS"
        resources = contents / "Resources"
        binaries.mkdir(parents=True)
        resources.mkdir()
        for binary in ("tech2-emu", "chipsoft-probe", "chipsoft-vin", "nano-usb-probe", "opensaab-security", "opensaab-reports", "opensaab-connection", "opensaab-mdi"):
            shutil.copy2(rust_target_dir / target_triple / "release" / binary, binaries / binary)
        run("xcrun", "swiftc", "-O", "-target", f"{architecture}-apple-macos12.0", "-framework", "AppKit", "-framework", "Security", "-framework", "CoreBluetooth", "-framework", "IOBluetooth", "-framework", "IOKit",
            SOURCES / "Launcher.swift", SOURCES / "ShellCore.swift", SOURCES / "NanoAdapterWorkflow.swift", SOURCES / "MdiAdapterWorkflow.swift", SOURCES / "PrivateAutomation.swift", SOURCES / "BundledRuntime.swift", SOURCES / "ShellChecks.swift", SOURCES / "DesignChecks.swift", SOURCES / "SecurityStatus.swift", SOURCES / "VehicleStatusView.swift", SOURCES / "UIParityChecks.swift", SOURCES / "USBAdapterDiscovery.swift", SOURCES / "USBAdapterChooser.swift", SOURCES / "USBChooserChecks.swift", SOURCES / "SecurityWorkflow.swift", SOURCES / "DtcReports.swift", SOURCES / "SupportReports.swift", SOURCES / "MenuShortcuts.swift", SOURCES / "EngineStartup.swift", SOURCES / "MacBluetoothTransport.swift", SOURCES / "MacBluetoothWorkflow.swift", SOURCES / "BluetoothActions.swift", SOURCES / "MacBluetoothChecks.swift", ROOT / "shared/apple-bluetooth/AppleBLETransport.swift", "-o", binaries / "OpenSAAB")
        shutil.copy2(EMULATOR / "ui/firmware-catalog.json", resources / "firmware-catalog.json")
        support_dir = resources / "system"
        support_dir.mkdir()
        shutil.copy2(EMULATOR / "ui/support-files.json", support_dir / "manifest.json")
        for name, source in support_inputs.items():
            shutil.copy2(source, support_dir / name)
        shutil.copy2(EMULATOR / "ui/interface-contract.json", resources / "interface-contract.json")
        shutil.copy2(EMULATOR / "ui/reporting-evidence-contract.json", resources / "reporting-evidence-contract.json")
        for binary in binaries.iterdir():
            if run("lipo", "-archs", binary, capture_output=True, text=True).stdout.strip() != architecture:
                raise SystemExit(f"Expected native {architecture}: {binary}")
        shutil.copy2(EMULATOR / "assets/branding/opensaab-t2-logo-official.png", resources / "opensaab.png")
        iconset = stage / "OpenSAAB.iconset"
        iconset.mkdir()
        for points in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                suffix = "@2x" if scale == 2 else ""
                run("sips", "-z", points * scale, points * scale,
                    EMULATOR / "assets/branding/opensaab-t2-logo-official.png",
                    "--out", iconset / f"icon_{points}x{points}{suffix}.png", stdout=subprocess.DEVNULL)
        run("iconutil", "-c", "icns", iconset, "-o", resources / "OpenSAAB.icns")
        display_name = f"OpenSAAB {VERSION} {'Intel' if architecture == 'x86_64' else 'Apple silicon'}"
        info = dict(CFBundleName=display_name, CFBundleDisplayName=display_name,
            CFBundleIdentifier="com.opensaab.desktop", CFBundleExecutable="OpenSAAB",
            CFBundlePackageType="APPL", CFBundleShortVersionString=VERSION,
            CFBundleVersion=VERSION, CFBundleIconFile="OpenSAAB.icns",
            LSMinimumSystemVersion="12.0", NSHighResolutionCapable=True, OpenSAABBuild=build_id(options.source_commit),
            NSBluetoothAlwaysUsageDescription="Read VIN and engine information using an explicitly selected, compatible Bluetooth adapter.")
        (contents / "Info.plist").write_bytes(plistlib.dumps(info))
        for file in ("LICENSE", "LICENSING.md", "THIRD_PARTY_NOTICES.md"):
            shutil.copy2(EMULATOR / file, resources / file)
        for file in ("OPENVCX_LGPL_3_0.txt", "OPENVCX_GPL_3_0.txt", "OPENVCX_SOURCE_ORIGIN.txt"):
            source = EMULATOR / "licenses" / file
            if not source.is_file():
                raise SystemExit(f"Missing private Nano initializer license/provenance: {source}")
            (resources / "licenses").mkdir(exist_ok=True)
            shutil.copy2(source, resources / "licenses" / file)
        shutil.copy2(ROOT / "shared/mdi-protocol/LICENSE",resources / "MDI_LICENSE.txt")
        shutil.copy2(ROOT / "shared/mdi-workflow/ANDROID_REFERENCE.json",resources / "mdi-android-reference.json")
        # Preserve license texts for all packages in this target's resolved graph.
        metadata = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1",
            "--filter-platform", target_triple, "--features", "load-test", "--manifest-path", EMULATOR / "Cargo.toml",
            capture_output=True, text=True).stdout)
        ids = {node["id"] for node in metadata["resolve"]["nodes"]}
        dependencies = []
        for package in metadata["packages"]:
            if package["id"] not in ids or not package["source"]:
                continue
            folder = Path(package["manifest_path"]).parent
            licenses = [p for p in folder.iterdir() if p.is_file() and
                        p.name.lower().startswith(("license", "licence", "copying", "notice", "copyright"))]
            # These upstream compatibility/parser crates omit the repository's
            # root license files from their crates.io archives. The resolved
            # defmt root carries the same repository and SPDX license identity.
            license_source = None
            if not licenses and (package["name"], package["version"]) in {("defmt", "0.3.100"), ("defmt-parser", "1.0.0")}:
                owner = next((p for p in metadata["packages"] if p["name"] == "defmt" and p["version"] == "1.1.1"
                    and p["repository"] == package["repository"] and p["license"] == package["license"]), None)
                if owner:
                    owner_folder = Path(owner["manifest_path"]).parent
                    licenses = [owner_folder / name for name in ("LICENSE-MIT", "LICENSE-APACHE")]
                    if not all(p.is_file() for p in licenses): licenses = []
                    else: license_source = "defmt-1.1.1 repository root (same repository and SPDX identity)"
            if not licenses:
                raise SystemExit(f"Missing dependency license text: {package['name']}")
            target = resources / "licenses" / f"{package['name']}-{package['version']}"
            target.mkdir(parents=True)
            for file in licenses:
                shutil.copy2(file, target / file.name)
            dependency = {key: package[key] for key in ("name", "version", "license")}
            if license_source: dependency["license_text_source"] = license_source
            dependencies.append(dependency)
        (resources / "dependencies.json").write_text(json.dumps(dependencies, indent=2) + "\n")
        try:
            commit = run("git", "-C", ROOT, "rev-parse", "HEAD", capture_output=True, text=True).stdout.strip()
            dirty = bool(run("git", "-C", ROOT, "status", "--porcelain", capture_output=True, text=True).stdout.strip())
            source_shape = "git_checkout"
        except subprocess.CalledProcessError:
            if not options.source_commit or len(options.source_commit) != 40 or any(c not in "0123456789abcdef" for c in options.source_commit):
                raise SystemExit("Private snapshot packaging requires --source-commit with the pinned predecessor SHA")
            commit, dirty, source_shape = options.source_commit, True, "private_modified_frozen_snapshot"
        provenance = dict(installed_program_name=artifact_name+".app", program_display_name=display_name, version=VERSION, platform="macos", architecture=architecture, source_commit=commit,
            source_dirty=dirty, firmware_included=bool(support_inputs), diagnostic_card_included=False,
            support_profile="android-approved" if support_inputs else "import-only",
            support_files=support if support_inputs else {}, signing="ad-hoc; not notarized",
            minimum_macos="12.0", rust_target=target_triple,
            private_candidate=True, source_shape=source_shape, nano_qualification="installed-app/hardware pending; CLI predecessor scope remains separate",
            mdi_workflow={"Android_reference":"0.1.0-preview.70","shared_package":"opensaab-mdi-workflow","USB_binding":"native-IOKit","installed_guest_qualification":"pending","reference_manifest_sha256":digest(ROOT / "shared/mdi-workflow/ANDROID_REFERENCE.json")},
            interface_contract_sha256=digest(resources / "interface-contract.json"),
            bluetooth_workflow={"package":"opensaab-vlinker-workflow", "version":"0.1.3", "source_pin_sha256":digest(ROOT / "shared/vlinker-workflow/SOURCE_SHA256")},
            apple_ble_source_sha256=digest(ROOT / "shared/apple-bluetooth/AppleBLETransport.swift"))
        (resources / "build.json").write_text(json.dumps(provenance, indent=2) + "\n")
        for binary in binaries.iterdir():
            run("codesign", "--force", "--sign", "-", binary)
        run("codesign", "--force", "--sign", "-", app)
        run("codesign", "--verify", "--deep", "--strict", app)
        # Preserve the exact signed bundle for private install and local checks.
        app_destination = output / (artifact_name + ".app")
        if app_destination.exists():
            raise SystemExit(f"App output already exists: {app_destination}")
        shutil.copytree(app, app_destination, symlinks=True)
        os.symlink("/Applications", disk / "Applications")
        label = "Apple silicon (ARM64)" if architecture == "arm64" else "Intel (x86_64)"
        install = (HERE / "INSTALL.txt").read_text().replace("@VERSION@", VERSION).replace("@ARCH@", label)
        (disk / "INSTALL.txt").write_text(install)
        run("hdiutil", "create", "-volname", artifact_name, "-srcfolder", disk,
            "-format", "UDZO", "-ov", destination)
        manifest = dict(provenance, artifact=destination.name, sha256=digest(destination),
            binaries={p.name: digest(p) for p in binaries.iterdir()})
        (output / (artifact_name + ".json")).write_text(json.dumps(manifest, indent=2) + "\n")
        (output / (artifact_name + ".sha256")).write_text(digest(destination) + "  " + destination.name + "\n")
        print(destination)

if __name__ == "__main__":
    main()
