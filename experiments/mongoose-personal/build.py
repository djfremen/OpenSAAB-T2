#!/usr/bin/env python3
"""Build only the scoped ARM64 Mongoose test APK, with its exact matching inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET
import zipfile

ROOT=Path(__file__).resolve().parent
REPO=ROOT.parents[1]
SDK=Path(os.environ.get('ANDROID_HOME',str(Path.home()/'Library/Android/sdk')))
JDK=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
BT=SDK/'build-tools/36.0.0'
JAR=SDK/'platforms/android-36/android.jar'
NDK=Path(os.environ.get('ANDROID_NDK_HOME',str(SDK/'ndk/android-ndk-r29')))
NDK_HOST='darwin-x86_64' if os.uname().sysname=='Darwin' else 'linux-x86_64'
CLANG=NDK/'toolchains/llvm/prebuilt'/NDK_HOST/'bin/aarch64-linux-android26-clang'
ENV=dict(os.environ,JAVA_HOME=str(JDK),PATH=str(JDK/'bin')+os.pathsep+os.environ.get('PATH',''),
         CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(CLANG))
NS='http://schemas.android.com/apk/res/android';ET.register_namespace('android',NS)
def a(key):return '{'+NS+'}'+key
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def run(*args):subprocess.run(list(map(str,args)),cwd=REPO,env=ENV,check=True)

def manifest(config,output):
    program=f"OpenSAAB {config['version']} {config['display_architecture']}"
    root=ET.Element('manifest',{'package':config['package'],a('versionCode'):str(config['version_code']),a('versionName'):config['version']})
    ET.SubElement(root,'uses-sdk',{a('minSdkVersion'):'26',a('targetSdkVersion'):'31'})
    ET.SubElement(root,'uses-feature',{a('name'):'android.hardware.usb.host',a('required'):'true'})
    ET.SubElement(root,'uses-permission',{a('name'):'android.permission.INTERNET'})
    app=ET.SubElement(root,'application',{a('label'):program,a('icon'):'@drawable/opensaab_logo',a('theme'):'@android:style/Theme.Material.NoActionBar',a('debuggable'):'false',a('allowBackup'):'false',a('extractNativeLibs'):'true'})
    for key,value in {'release_date':config['release_date'],'architecture':config['display_architecture'],'source_revision':config['source_revision'],'support_url':'https://ko-fi.com/djfremen','security_access_mode':'unavailable'}.items():ET.SubElement(app,'meta-data',{a('name'):key,a('value'):value})
    activity=ET.SubElement(app,'activity',{a('name'):'com.opensaab.usb.MongooseUsbActivity',a('label'):program,a('exported'):'true'})
    intent=ET.SubElement(activity,'intent-filter');ET.SubElement(intent,'action',{a('name'):'android.intent.action.MAIN'});ET.SubElement(intent,'category',{a('name'):'android.intent.category.LAUNCHER'})
    ET.SubElement(app,'provider',{a('name'):'com.opensaab.usb.SupportReportProvider',a('authorities'):config['package']+'.support-reports',a('exported'):'false',a('grantUriPermissions'):'true'})
    ET.ElementTree(root).write(output,encoding='utf-8',xml_declaration=True)
    return program

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--worker',type=Path);parser.add_argument('--source-revision');args=parser.parse_args()
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
    config=json.loads((ROOT/'release.json').read_text())
    assert re.fullmatch(r'\d+\.\d+\.\d+-preview\.\d+',config['version'])
    assert config['architecture']=='arm64' and config['security_access']['mode']=='unavailable'
    assert re.fullmatch(r'\d{4}-\d{2}-\d{2}',config['release_date'])
    revision=args.source_revision or subprocess.check_output(['git','-C',str(REPO),'rev-parse','HEAD'],text=True).strip()
    assert re.fullmatch(r'[0-9a-f]{40}',revision)
    config['source_revision']=revision
    if args.worker:worker=args.worker.resolve()
    else:
        run('cargo','rustc','--locked','--manifest-path',ROOT/'worker/Cargo.toml','--target','aarch64-linux-android','--release','--bin','mongoose-usb-probe','--','-C','link-arg=-Wl,-z,max-page-size=16384')
        worker=ROOT/'worker/target/aarch64-linux-android/release/mongoose-usb-probe'
    data=worker.read_bytes();assert data[:5]==b'\x7fELF\x02' and int.from_bytes(data[18:20],'little')==183
    work=out/'build';work.mkdir();classes=work/'classes';classes.mkdir()
    xml=work/'AndroidManifest.xml';program=manifest(config,xml)
    resources=work/'resources.zip';unsigned=work/'unsigned.apk'
    run(BT/'aapt2','compile','--dir',ROOT/'android/res','-o',resources)
    run(BT/'aapt2','link','-o',unsigned,'--manifest',xml,'-I',JAR,'-R',resources)
    sources=sorted((ROOT/'android/java').rglob('*.java'))
    run(JDK/'bin/javac','-source','8','-target','8','-cp',JAR,'-d',classes,*sources)
    run(BT/'d8','--min-api','26','--lib',JAR,'--output',work,*sorted(classes.rglob('*.class')))
    inputs={str(p.relative_to(ROOT)):sha(p) for p in sorted(ROOT.rglob('*')) if p.is_file() and not any(x in p.relative_to(ROOT).parts for x in ['target','build','__pycache__'])}
    build={'product':config['product'],'version':config['version'],'version_code':config['version_code'],'release_date':config['release_date'],'platform':'android','architecture':'arm64','program_name':program,'source_revision':revision,'worker_sha256':sha(worker),'input_sha256':inputs,'scope':config['scope'],'security_access':config['security_access'],'hardware_tested':False}
    with zipfile.ZipFile(unsigned,'a') as apk:
        for dex in sorted(work.glob('classes*.dex')):apk.write(dex,dex.name)
        apk.write(worker,'lib/arm64-v8a/libmongoose_probe.so')
        apk.writestr('assets/build.json',json.dumps(build,indent=2))
        apk.write(ROOT/'interface-contract.json','assets/interface-contract.json')
        for p in sorted((ROOT/'worker/LICENSES').glob('*')):
            if p.is_file():apk.write(p,'assets/legal/'+p.name)
    aligned=work/'aligned.apk';run(BT/'zipalign','-f','4',unsigned,aligned)
    signing=Path(os.environ.get('OPENSAAB_SIGNING_HOME',str(Path.home()/'.local/share/opensaab/release-signing')))
    package=out/config['installer_name']
    run(BT/'apksigner','sign','--ks',signing/'opensaab-release.p12','--ks-key-alias','opensaab-release','--ks-pass','file:'+str(signing/'password.txt'),'--out',package,aligned)
    run(BT/'apksigner','verify','--verbose','--print-certs',package)
    receipt={**build,'artifact':package.name,'artifact_sha256':sha(package),'artifact_bytes':package.stat().st_size,'nondebuggable':True,'minimum_android_api':26,
             'toolchain':{'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'android_build_tools':'36.0.0','android_platform':'36','NDK':'r29','clang_sha256':sha(CLANG),'android_jar_sha256':sha(JAR),'aapt2_sha256':sha(BT/'aapt2'),'d8_sha256':sha(BT/'d8'),'javac_sha256':sha(JDK/'bin/javac')}}
    (out/'BUILD-RECEIPT.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({'artifact':str(package),'sha256':receipt['artifact_sha256'],'version':config['version']}))

if __name__=='__main__':main()
