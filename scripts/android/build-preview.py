#!/usr/bin/env python3
"""Build a clean, versioned ARM64 preview and retain all source/input/native receipts.

This recipe rebuilds the root/MX, recovered and MDI firmware engines, connection
worker and MDI carrier. Only the
explicit historical workers in preview-build.json may be retained. It neither
installs the result nor claims an independent byte-for-byte rebuild.
"""
import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

from build_profiles import PROFILES, validate_elf
from native_payload_inputs import load_inputs

REPO = Path(__file__).resolve().parents[2]
TARGET = 'aarch64-linux-android'


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def capture(*command, env=None):
    return subprocess.check_output(list(map(str, command)), cwd=REPO, env=env, text=True,
                                   stderr=subprocess.STDOUT).strip()


def source_revision():
    if capture('git', 'status', '--porcelain'):
        raise ValueError('Preview packaging requires a clean committed source checkout')
    return capture('git', 'rev-parse', 'HEAD')


def tree_receipt(directory):
    """Hash actual dependency/tool files, including names and contents."""
    directory = Path(directory)
    files = []
    for path in sorted(directory.rglob('*')):
        relative = path.relative_to(directory)
        if any(p in ('target', '.git') for p in relative.parts) or not path.is_file():
            continue
        files.append({'file': relative.as_posix(), 'sha256': sha(path), 'bytes': path.stat().st_size})
    digest = hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()
    return {'sha256': digest, 'files': files}


def require_engine_version(manifest, version):
    import re
    package = Path(manifest).read_text().split('[package]', 1)[1].split('\n[', 1)[0]
    match = re.search(r'^version\s*=\s*"([^"]+)"', package, re.M)
    if not match or match[1] != version:
        raise ValueError('Engine Cargo version must match the preview: ' + str(manifest))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New directory outside the source checkout')
    retained = parser.add_mutually_exclusive_group(required=True)
    retained.add_argument('--retained-apk', type=Path, help='Exact historical APK pinned by preview-build.json')
    retained.add_argument('--retained-native-dir', type=Path, help='Directory containing the exact two pinned native members under lib/arm64-v8a')
    parser.add_argument('--support-root', type=Path, help='Optional authorized support files; default output is firmware-free')
    args = parser.parse_args()
    commit = source_revision()
    spec_path = REPO / 'android/tech2-app/preview-build.json'
    spec = json.loads(spec_path.read_text())
    if spec.get('schema') != 1 or spec.get('architecture') != 'arm64-v8a':
        raise ValueError('Unsupported preview build specification')
    version = spec['application_version']
    output = args.output.resolve()
    if output == REPO or REPO in output.parents or output.exists():
        raise ValueError('Use a new output directory outside the source checkout; historical output is never overwritten')
    manifests = {
        'root': REPO / 'Cargo.toml',
        'recovered': REPO / 'core-recovered/emulator/Cargo.toml',
        'mdi': REPO / 'core-mdi/mdi-firmware-engine/OpenSAAB-T2/Cargo.toml',
        'mdi-carrier': REPO / 'android/adapters/mdi/native/Cargo.toml',
    }
    for manifest in manifests.values():
        require_engine_version(manifest, version)
    if json.loads((REPO / 'core-recovered/emulator/ui/recovered-core-identity.json').read_text())['version'] != version:
        raise ValueError('Recovered runtime packaged identity differs from the preview')
    if args.retained_apk and sha(args.retained_apk) != spec['retained_package']['sha256']:
        raise ValueError('Retained APK differs from the reviewed source package pin')
    sdk = Path(os.environ.get('ANDROID_HOME', Path.home() / 'Library/Android/sdk'))
    ndk = Path(os.environ.get('ANDROID_NDK_HOME', sdk / 'ndk/android-ndk-r29'))
    jdk = Path(os.environ.get('JAVA_HOME', '/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
    gradle_java = os.environ.get('OPENSAAB_GRADLE_JAVA_HOME')
    if not gradle_java:
        gradle_java = capture('/usr/libexec/java_home', '-v', '21')
    host = 'darwin-x86_64' if sys.platform == 'darwin' else 'linux-x86_64'
    linker = ndk / 'toolchains/llvm/prebuilt' / host / 'bin/aarch64-linux-android26-clang'
    if not linker.is_file():
        raise ValueError('Missing Android NDK linker: ' + str(linker))
    output.mkdir(parents=True)
    (output / 'logs').mkdir()
    env = dict(os.environ, ANDROID_HOME=str(sdk), ANDROID_NDK_HOME=str(ndk), JAVA_HOME=str(jdk),
               OPENSAAB_GRADLE_JAVA_HOME=gradle_java,
               CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(linker),
               RUSTFLAGS='--remap-path-prefix=' + str(REPO) + '=/opensaab --remap-path-prefix=' + str(Path.home() / '.cargo') + '=/cargo')
    env.pop('OPENSAAB_PINNED_NATIVE', None)
    env.pop('OPENSAAB_MDI_MODULE', None)
    env.pop('OPENSAAB_MDI_BRIDGE', None)
    env['OPENSAAB_BUNDLE_SUPPORT'] = '1' if args.support_root else '0'
    if args.support_root:
        env['OPENSAAB_SUPPORT_ROOT'] = str(args.support_root.resolve())
    # Signing secrets are passed by the caller through files and never copied to receipts.
    for name in ('OPENSAAB_RELEASE_KEYSTORE', 'OPENSAAB_RELEASE_PASSWORD_FILE'):
        if not env.get(name) or not Path(env[name]).is_file():
            raise ValueError('Set ' + name + ' to an existing private signing input')
    commands = []

    def run(label, command, run_env=env):
        commands.append({'step': label, 'command': list(map(str, command))})
        print(label, flush=True)
        with (output / 'logs' / (label + '.txt')).open('w') as log:
            subprocess.run(list(map(str, command)), cwd=REPO, env=run_env, stdout=log,
                           stderr=subprocess.STDOUT, check=True)

    toolchain = {
        'host': {'platform': platform.platform(), 'machine': platform.machine()},
        'python': {'version': sys.version, 'executable_sha256': sha(sys.executable)},
        'rustc': capture('rustc', '-Vv'), 'cargo': capture('cargo', '-Vv'),
        'java': capture(jdk / 'bin/java', '-version'),
        'javac': capture(jdk / 'bin/javac', '-version'),
        'gradle_java': capture(Path(gradle_java) / 'bin/java', '-version'),
        'ndk_properties': (ndk / 'source.properties').read_text(),
        'ndk_linker': {'sha256': sha(linker), 'version': capture(linker, '--version')},
        'ndk_native_tools': {name: sha(linker.parent / name) for name in ('clang', 'ld.lld')},
        'ndk_target_libraries': tree_receipt(linker.parent.parent / 'sysroot/usr/lib/aarch64-linux-android/26'),
        'rustc_executable_sha256': sha(capture('rustup', 'which', 'rustc')),
        'cargo_executable_sha256': sha(capture('rustup', 'which', 'cargo')),
        'rust_target_libraries': tree_receipt(Path(capture('rustc', '--print', 'sysroot')) / 'lib/rustlib' / TARGET / 'lib'),
        'android_jar_sha256': sha(sdk / 'platforms/android-36/android.jar'),
        'build_tools': tree_receipt(sdk / 'build-tools/36.0.0'),
        'wrapper_properties_sha256': sha(REPO / 'gradle/wrapper/gradle-wrapper.properties'),
        'wrapper_jar_sha256': sha(REPO / 'gradle/wrapper/gradle-wrapper.jar'),
        'rustflags': env['RUSTFLAGS'],
        'reproducibility': 'Tool/input receipts retained; independent byte-for-byte second rebuild not performed',
    }
    write_json(output / 'toolchain-receipt.json', toolchain)
    native = {'schema': 1, 'application_version': version, 'architecture': 'arm64-v8a',
              'source_revision': commit, 'components': {}}
    historical_sources=[]
    origins=sorted({(item['source_revision'],item['source_application_version'])
                    for item in spec['retained_components'].values()})
    for original_commit,original_version in origins:
        capture('git','cat-file','-e',original_commit+'^{commit}')
        original_archive=output/('OpenSAAB-'+original_version+'-retained-native-source-'+original_commit+'.tar.gz')
        with original_archive.open('wb') as stream:
            subprocess.run(['git','archive','--format=tar.gz',original_commit],cwd=REPO,stdout=stream,check=True)
        historical_sources.append({'source_revision':original_commit,'application_version':original_version,
            'archive':original_archive.name,'sha256':sha(original_archive),
            'components':[name for name,item in spec['retained_components'].items() if item['source_revision']==original_commit]})
    with zipfile.ZipFile(args.retained_apk) if args.retained_apk else _no_archive() as retained_apk:
        for name, item in spec['retained_components'].items():
            data = retained_apk.read(name) if retained_apk else (args.retained_native_dir / name).read_bytes()
            validate_elf(data[:20], PROFILES['arm64'])
            if hashlib.sha256(data).hexdigest() != item['sha256']:
                raise ValueError('Retained native member differs from source pin: ' + name)
            destination = output / 'native-components' / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
            native['components'][name] = {**item, 'kind': 'retained',
                'path': str(destination.relative_to(output)),
                'retained_from_application_version': spec['retained_package']['application_version'],
                'retained_from_apk_sha256': spec['retained_package']['sha256']}
    dependencies = []
    registry_sources = {}
    for role, manifest in manifests.items():
        build_env = dict(env, CARGO_TARGET_DIR=str(output / 'cargo' / role))
        metadata = json.loads(capture('cargo', 'metadata', '--locked', '--no-default-features',
                                     '--filter-platform', TARGET, '--format-version', '1',
                                     '--manifest-path', manifest, env=build_env))
        packages = []
        for package in metadata['packages']:
            package_root = Path(package['manifest_path']).parent
            if package['source']:
                registry_sources[package['id']] = (package_root, package['name'] + '-' + package['version'] + '-' + hashlib.sha256(package['source'].encode()).hexdigest()[:8])
            # Workspace/local source bytes are covered by the immutable Git source archive.
            packages.append({'name': package['name'], 'version': package['version'],
                             'source': package['source'],
                             'manifest_sha256': sha(package['manifest_path']),
                             **({'source_tree': tree_receipt(package_root)} if package['source'] else {})})
        dependencies.append({'engine': role, 'cargo_lock_sha256': sha(manifest.with_name('Cargo.lock')),
                             'packages': packages})
        cargo_output = ['--lib'] if role=='mdi-carrier' else ['--bin','tech2-emu']
        if role=='recovered':cargo_output.extend(['--bin','opensaab-connection'])
        run('build-' + role, ['cargo', 'build', '--manifest-path', manifest, '--locked', '--release',
                             '--no-default-features', '--target', TARGET, *cargo_output], build_env)
        outputs = {'root': {'libtech2_emu.so':'tech2-emu', 'libtech2_mx.so':'tech2-emu'},
                   'recovered': {'libtech2_recovered.so':'tech2-emu', 'libopensaab_connection.so':'opensaab-connection'},
                   'mdi': {'libtech2_mdi.so':'tech2-emu'},
                   'mdi-carrier': {'libopensaab_mdi_android.so':'libopensaab_mdi_android.so'}}[role]
        for basename,executable in outputs.items():
            source = output / 'cargo' / role / TARGET / 'release' / executable
            name = 'lib/arm64-v8a/' + basename
            destination = output / 'native-components' / name
            shutil.copy2(source, destination)
            native['components'][name] = {'kind': 'built', 'sha256': sha(destination),
                'path': str(destination.relative_to(output)), 'source_revision': commit,
                'source_application_version': version, 'source_manifest': str(manifest.relative_to(REPO)),
                'source_binary':executable,
                'capabilities': ('Rebuilt MDI carrier with strict child-status reporting' if role=='mdi-carrier' else
                                 'Rebuilt shared connection worker with owned MX+ readiness' if executable=='opensaab-connection' else
                                 'Rebuilt firmware engine') + '; installed and physical adapter verification pending'}
    write_json(output / 'cargo-dependency-receipt.json', {'schema': 1, 'engines': dependencies})
    dependency_archive=output/('OpenSAAB-'+version+'-cargo-source-inputs.tar.gz')
    with tarfile.open(dependency_archive,'w:gz') as archive:
        for source,archive_name in sorted(registry_sources.values(),key=lambda item:item[1]):
            archive.add(source,arcname=archive_name,filter=lambda info: None if any(part in ('target','.git') for part in Path(info.name).parts) else info)
    native_path = output / 'native-inputs.json'
    write_json(native_path, native)
    _, provenance = load_inputs(native_path, version, commit)
    write_json(output / 'native-components.json', provenance)
    run('package', [sys.executable, REPO / 'scripts/android/build-tech2-app.py', '--release',
                   '--private-preview', '--release-date', spec['release_date'], '--version-name', version,
                   '--version-code', spec['version_code'], '--native-input-manifest', native_path,
                   '--output-dir', output / 'package'])
    if source_revision() != commit:
        raise ValueError('Source changed during the build; artifact cannot receive a clean source receipt')
    packaged = output / 'package' / ('OpenSAAB-T2-' + version + '-arm64-v8a.apk')
    apk = output / ('OpenSAAB-' + version + '-android-arm64-v8a.apk')
    shutil.copy2(packaged, apk)
    with zipfile.ZipFile(apk) as archive:
        for name, item in native['components'].items():
            if hashlib.sha256(archive.read(name)).hexdigest() != item['sha256']:
                raise ValueError('Final APK native component changed: ' + name)
    run('signature', [sdk / 'build-tools/36.0.0/apksigner', 'verify', '--print-certs', apk])
    run('packaged-identity', [sdk / 'build-tools/36.0.0/aapt', 'dump', 'badging', apk])
    source_archive = output / ('OpenSAAB-' + version + '-matching-source.tar.gz')
    with source_archive.open('wb') as stream:
        subprocess.run(['git', 'archive', '--format=tar.gz', commit], cwd=REPO, stdout=stream, check=True)
    native_archive = output / ('OpenSAAB-' + version + '-android-arm64-v8a-native-components.tar.gz')
    with tarfile.open(native_archive, 'w:gz') as archive:
        archive.add(output / 'native-components', arcname='native-components')
        archive.add(output / 'native-components.json', arcname='native-components.json')
    write_json(output / 'build-commands.json', {'schema': 1, 'source_revision': commit, 'commands': commands})
    receipt_paths = ['toolchain-receipt.json', 'cargo-dependency-receipt.json', 'native-components.json',
                     'build-commands.json', 'package/report-dependency-receipt.json']
    manifest = {'schema': 1, 'product': 'OpenSAAB', 'version': version, 'application_version': version, 'version_code': spec['version_code'],
                'platform': 'android', 'artifact_sha256': sha(apk),
                'security_access': {'policy': 'security-access-password-1', 'mode': 'no_operator_password'},
                'capabilities': ['Android session lifecycle and outcome reporting source fixes',
                                 'Rebuilt common/MX, recovered and MDI firmware engines plus MDI carrier and shared connection worker',
                                 'Explicit unchanged Nano/Chipsoft probes'],
                'architecture': 'arm64-v8a', 'source_revision': commit, 'source_dirty': False,
                'build_spec_sha256': sha(spec_path), 'installed_name': 'OpenSAAB ' + version + ' ARM64',
                'session_lifecycle_contract_sha256': sha(REPO/'android/tech2-app/session-lifecycle-contract.json'),
                'published': False, 'native_components': provenance['components'],
                'retained_component_sources': historical_sources,
                'artifacts': {p.name: {'sha256': sha(p), 'bytes': p.stat().st_size}
                              for p in (apk, source_archive, native_archive, dependency_archive,
                                        *(output/item['archive'] for item in historical_sources))},
                'receipts': {p: sha(output / p) for p in receipt_paths},
                'logs': {str(p.relative_to(output)): sha(p) for p in sorted((output/'logs').glob('*.txt'))},
                'verification': {'source_and_all_native_hashes': 'verified',
                                 'signature': 'verified', 'java_three_native_engines_mdi_carrier_and_connection': 'built',
                                 'independent_byte_for_byte_rebuild': 'not performed',
                                 'installed_target': 'not performed', 'physical_adapters': 'not performed'}}
    write_json(output / 'candidate-manifest.json', manifest)
    write_json(output / ('OpenSAAB-' + version + '-android-arm64-v8a-manifest.json'), manifest)
    print(json.dumps({'artifact': str(apk), 'sha256': sha(apk), 'source_revision': commit}))


class _no_archive:
    def __enter__(self):
        return None

    def __exit__(self, *args):
        return False


if __name__ == '__main__':
    main()
