#!/usr/bin/env python3
"""Create a persistent local release key, without overwriting any existing key material."""
import os, secrets, subprocess
from pathlib import Path
root=Path.home()/'.local/share/opensaab/release-signing'
root.mkdir(parents=True,exist_ok=True,mode=0o700);root.chmod(0o700)
key=root/'opensaab-release.p12';password=root/'password.txt'
if key.exists() or password.exists():
    if not key.is_file() or not password.is_file():raise SystemExit('Incomplete signing directory; inspect it before proceeding. Nothing overwritten.')
    print('Existing release signing material retained:',root)
else:
    fd=os.open(password,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w') as f:f.write(secrets.token_urlsafe(48)+'\n')
    jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))
    subprocess.run([str(jdk/'bin/keytool'),'-genkeypair','-keystore',str(key),'-storetype','PKCS12','-storepass:file',str(password),'-keypass:file',str(password),'-alias','opensaab-release','-keyalg','RSA','-keysize','4096','-validity','10000','-dname','CN=OpenSAAB T2, O=OpenSAAB'],check=True)
    key.chmod(0o600)
    subprocess.run([str(jdk/'bin/keytool'),'-exportcert','-rfc','-keystore',str(key),'-storepass:file',str(password),'-alias','opensaab-release','-file',str(root/'certificate.pem')],check=True)
    print('Created local release key:',root)
print('Back up the key and password securely off this Mac before a public release. Never commit them.')
