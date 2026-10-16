#!/usr/bin/env python3
"""Build a scoped portable Windows preview from clean matching source on macOS.

The baseline supplies the separately pinned vendor comparison helpers, approved
support images and notices. Six Rust executables and the WinForms shell are rebuilt.
No diagnostic card, private state or test output is placed in the public ZIP.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--baseline', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    root = Path(__file__).resolve().parents[2]
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise SystemExit('Build requires a clean reviewed source snapshot')
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    version = json.loads((root/'emulator/installer/windows/x64/build-version.json').read_text())
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    name = 'OpenSAAB-' + version['version'] + '-windows-x86_64'
    stage = output/name
    build = output/'private-build'
    build.mkdir()
    log = (build/'build.log').open('w')
    def run(command, **kwargs):
        log.write('COMMAND ' + ' '.join(map(str, command)) + '\n'); log.flush()
        subprocess.run(list(map(str, command)), check=True, stdout=log, stderr=subprocess.STDOUT, **kwargs)
    if sha(args.baseline) != 'c27b247c390a45f53cc963e7dfd5ec2f438472376137164588cad90e6ca5d173':
        raise SystemExit('Baseline does not match the published Windows 0.2.17 package')
    stage.mkdir()
    with zipfile.ZipFile(args.baseline) as z:
        for info in z.infolist():
            parts = Path(info.filename).parts
            if info.is_dir(): continue
            if not parts or '..' in parts or Path(info.filename).is_absolute():
                raise SystemExit('Unsafe baseline archive path')
            relative = Path(*parts[1:])
            target = stage/relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(z.read(info))
    metadata = {**version, 'source_revision': revision, 'channel': 'preview'}
    meta = build/'opensaab-build.json'
    meta.write_text(json.dumps(metadata, indent=2)+'\n')
    bins = ['tech2-emu', 'opensaab-security', 'opensaab-reports', 'chipsoft-probe', 'chipsoft-vin', 'opensaab-connection']
    env = {**os.environ, 'CARGO_TARGET_DIR': str(build/'cargo'),
           'CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER': 'x86_64-w64-mingw32-gcc'}
    command = ['cargo', 'build', '--manifest-path', root/'emulator/Cargo.toml', '--locked', '--release',
               '--no-default-features', '--features', 'load-test', '--target', 'x86_64-pc-windows-gnu']
    for binary in bins: command += ['--bin', binary]
    run(command, env=env)
    for binary in bins:
        shutil.copy2(build/'cargo/x86_64-pc-windows-gnu/release'/(binary+'.exe'), stage/(binary+'.exe'))
    project = root/'emulator/installer/windows/x64/OpenSAAB.csproj'
    run(['dotnet', 'restore', project, '--locked-mode', '-p:RuntimeIdentifier='])
    run(['dotnet', 'build', project, '--no-restore', '-c', 'Release', '-o', stage,
         '-p:RuntimeIdentifier=', '-p:BuildMetadataPath='+str(meta)])
    tests = build/'runtime-checks'
    run(['dotnet', 'build', project, '--no-restore', '-c', 'Release', '-o', tests,
         '-p:RuntimeIdentifier=', '-p:BuildMetadataPath='+str(meta), '-p:IncludeRuntimeChecks=true',
         '-p:StartupObject=RuntimeChecks', '-p:AssemblyName=OpenSAAB-RuntimeChecks-'+version['version']+'-x86_64'])
    for obsolete in ['OpenSAAB.exe', 'OpenSAAB.exe.config', 'payload-manifest.json', 'SHA256SUMS.txt', 'README.md', 'test-bluetooth.ps1']:
        (stage/obsolete).unlink(missing_ok=True)
    # Cargo's locked resolved sources supply notices for these rebuilt workers.
    dependency_json = build/'cargo-metadata.json'
    with dependency_json.open('w') as target:
        subprocess.run(['cargo', 'metadata', '--manifest-path', str(root/'emulator/Cargo.toml'),
                        '--locked', '--format-version', '1', '--no-default-features',
                        '--features', 'load-test', '--filter-platform', 'x86_64-pc-windows-gnu'],
                       env=env, check=True, stdout=target, stderr=log)
    dependencies = json.loads(dependency_json.read_text())
    notices = []
    for package in dependencies['packages']:
        if package.get('source') is None: continue
        origin = Path(package['manifest_path']).parent
        notice_dir = stage/'licenses'/(package['name']+'-'+package['version'])
        copied = []
        for source in sorted(origin.iterdir()):
            if source.is_file() and source.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE')):
                notice_dir.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, notice_dir/source.name)
                copied.append(source.name)
        notices.append({'name': package['name'], 'version': package['version'],
                        'license': package.get('license'), 'notice_files': copied})
    (stage/'licenses/rebuilt-cargo-dependencies.json').write_text(json.dumps(notices, indent=2)+'\n')
    (stage/'interface-provenance.json').write_text(json.dumps({'source_revision':revision,
        'interface_contract_sha256':sha(root/'emulator/ui/interface-contract.json'),
        'scope':'Scoped Windows preview; complete common-interface qualification pending.'}, indent=2)+'\n')
    (stage/'windows-dependencies.json').write_text(json.dumps({'version':version['version'],
        'source_revision':revision, 'target':'x86_64-pc-windows-gnu',
        'rust_executables_rebuilt':bins, 'shell_target':'.NET Framework 4.8 x64',
        'shell_sdk_contracts':'10.0.22621.755', 'baseline_sha256':sha(args.baseline),
        'retained_vendor_bridges':['chipsoft-native-bridge.exe','vcx-native-bridge.exe'],
        'vendor_driver_qualification':'Physical Chipsoft and VCX driver tests pending.'}, indent=2)+'\n')
    for binary in bins:
        imports = subprocess.check_output(['x86_64-w64-mingw32-objdump', '-p', str(stage/(binary+'.exe'))], text=True)
        (build/(binary+'-pe-imports.txt')).write_text(imports)
        for line in imports.splitlines():
            if 'DLL Name:' in line:
                dll = line.split('DLL Name:')[1].strip()
                if dll.lower().startswith(('libgcc', 'libstdc++', 'libwinpthread')) and not (stage/dll).exists():
                    raise SystemExit('Unbundled MinGW runtime dependency: '+dll)
    for debug in stage.glob('*.pdb'): debug.unlink()
    if (stage/'runtime/Saab NAO.bin').exists(): raise SystemExit('Diagnostic card unexpectedly included')
    (stage/'opensaab-build.json').write_text(meta.read_text())
    readme = root/'emulator/installer/windows/x64/PREVIEW_README.md'
    shutil.copy2(readme, stage/'README.md')
    manifest = [{'file':str(f.relative_to(stage)), 'bytes':f.stat().st_size, 'sha256':sha(f)}
                for f in sorted(stage.rglob('*')) if f.is_file()]
    (stage/'payload-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    (stage/'SHA256SUMS.txt').write_text(''.join(sha(f)+'  '+str(f.relative_to(stage))+'\n' for f in sorted(stage.rglob('*')) if f.is_file() and f.name != 'SHA256SUMS.txt'))
    package = output/(name+'.zip')
    with zipfile.ZipFile(package, 'w', zipfile.ZIP_DEFLATED, compresslevel=9, strict_timestamps=False) as z:
        for f in sorted(stage.rglob('*')):
            if f.is_file(): z.write(f, name+'/'+str(f.relative_to(stage)))
    (build/'build-result.json').write_text(json.dumps({'metadata':metadata, 'baseline_sha256':sha(args.baseline),
        'package':package.name, 'package_sha256':sha(package), 'rebuilt_rust_executables':bins}, indent=2)+'\n')
    print(package)


if __name__ == '__main__': main()
