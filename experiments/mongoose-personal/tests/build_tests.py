#!/usr/bin/env python3
"""Build the signed installed-package test companion, never a distribution APK."""
import argparse,os,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
SDK=Path.home()/'Library/Android/sdk';BT=SDK/'build-tools/36.0.0';JAR=SDK/'platforms/android-36/android.jar';JDK=Path('/Applications/Android Studio.app/Contents/jbr/Contents/Home');ENV=dict(os.environ,JAVA_HOME=str(JDK),PATH=str(JDK/'bin')+':'+os.environ['PATH'])
def run(*a):subprocess.run(list(map(str,a)),check=True,env=ENV)
p=argparse.ArgumentParser();p.add_argument('--app-classes',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args();a.output.mkdir(parents=True);classes=a.output/'classes';classes.mkdir()
manifest=a.output/'AndroidManifest.xml';manifest.write_text('''<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.opensaab.mongoosepersonaltests"><uses-sdk android:minSdkVersion="26" android:targetSdkVersion="31"/><application android:label="OpenSAAB Mongoose installed tests"/><instrumentation android:name="com.opensaab.mongoosepersonaltests.MongoosePersonalInstrumentation" android:targetPackage="com.opensaab.tech2.mongoosebeta"/></manifest>''')
run(BT/'aapt2','link','-o',a.output/'unsigned.apk','--manifest',manifest,'-I',JAR)
run(JDK/'bin/javac','-source','8','-target','8','-cp',str(JAR)+':'+str(a.app_classes),'-d',classes,ROOT/'MongoosePersonalInstrumentation.java')
run(BT/'d8','--min-api','26','--lib',JAR,'--output',a.output,*classes.rglob('*.class'))
import zipfile
with zipfile.ZipFile(a.output/'unsigned.apk','a') as z:z.write(a.output/'classes.dex','classes.dex')
run(BT/'zipalign','-f','4',a.output/'unsigned.apk',a.output/'aligned.apk')
signing=Path.home()/'.local/share/opensaab/release-signing'
run(BT/'apksigner','sign','--ks',signing/'opensaab-release.p12','--ks-key-alias','opensaab-release','--ks-pass','file:'+str(signing/'password.txt'),'--out',a.output/'installed-tests.apk',a.output/'aligned.apk')
