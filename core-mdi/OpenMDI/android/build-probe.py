#!/usr/bin/env python3
"""Build the private OpenMDI ARM64 Android transport research APK."""
import os, pathlib, shutil, subprocess, zipfile, hashlib, json
root=pathlib.Path(__file__).resolve().parent
sdk=pathlib.Path(os.environ.get('ANDROID_HOME',str(pathlib.Path.home()/'Library/Android/sdk')))
jdk=pathlib.Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
bt=sdk/'build-tools/36.0.0'; jar=sdk/'platforms/android-36/android.jar'
out=root/'build'; out.mkdir(exist_ok=True)
classes=out/'classes'; shutil.rmtree(classes,ignore_errors=True); classes.mkdir()
env=dict(os.environ,JAVA_HOME=str(jdk),PATH=str(jdk/'bin')+os.pathsep+os.environ['PATH'])
def run(*args): subprocess.run([str(x) for x in args],env=env,check=True)
run(jdk/'bin/javac','-source','8','-target','8','-cp',jar,'-d',classes,*sorted((root/'java').rglob('*.java')))
run(bt/'d8','--min-api','28','--lib',jar,'--output',out,*sorted(classes.rglob('*.class')))
run(bt/'aapt','package','-f','-M',root/'AndroidManifest.xml','-I',jar,'-F',out/'unsigned.apk')
native=root/'probe/target/aarch64-linux-android/release/libopenmdi_android_probe.so'
if not native.exists(): raise SystemExit('Build Android ARM64 native probe first')
with zipfile.ZipFile(out/'unsigned.apk','a') as z:
    executable=root.parent.parent/'OpenSAAB-T2/target/aarch64-linux-android/release/tech2-emu'
    if not executable.is_file():raise SystemExit("Build original MDI firmware executable first")
    z.write(executable,'lib/arm64-v8a/libtech2_mdi.so')
    for legal_root in [root.parent,root.parent.parent/'OpenSAAB-T2']:
        for name in ['LICENSE','LICENSING.md','THIRD_PARTY_NOTICES.md']:
            if (legal_root/name).is_file():z.write(legal_root/name,'assets/legal/'+legal_root.name+'/'+name)
        if (legal_root/'licenses').is_dir():
            for f in sorted((legal_root/'licenses').rglob('*')):
                if f.is_file():z.write(f,'assets/legal/'+legal_root.name+'/licenses/'+str(f.relative_to(legal_root/'licenses')))
    z.write(out/'classes.dex','classes.dex'); z.write(native,'lib/arm64-v8a/libopenmdi_android_probe.so')
run(bt/'zipalign','-f','-p','4',out/'unsigned.apk',out/'aligned.apk')
run(bt/'apksigner','sign','--ks',pathlib.Path.home()/'.android/debug.keystore','--ks-pass','pass:android','--key-pass','pass:android','--out',out/'openmdi-research.apk',out/'aligned.apk')
run(bt/'apksigner','verify',out/'openmdi-research.apk')
pins={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [out/'openmdi-research.apk',native]}
(out/'artifact-pins.json').write_text(json.dumps(pins,indent=2)+'\n'); print(json.dumps(pins))
