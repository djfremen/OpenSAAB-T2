#!/usr/bin/env python3
"""One synthetic corpus for Python, Swift and C# status readers. No vehicle I/O."""
import hashlib, importlib.util, json, os, shutil, subprocess, sys, tempfile, unittest
from datetime import datetime, timezone
from unittest.mock import patch
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('security_status',ROOT/'installer/linux/x86_64/security_status.py')
status=importlib.util.module_from_spec(spec);spec.loader.exec_module(status)

def cases():
    vin=b'YS3FD49YX41000001'
    cleared=bytes([255])*714
    pre=bytearray(cleared);pre[20:37]=vin
    post=bytearray(pre);post[38:46]=bytes(range(1,9))
    digest=lambda b:hashlib.sha256(b).hexdigest()
    receipt=dict(schema='1',vehicle_hash=digest(vin),stage='imported',started_utc='2026-09-23T00:00:00Z',processed_utc='2026-09-23T00:00:01Z',imported_utc='2026-09-23T00:00:02Z',ssa_hash=digest(post))
    out=[]
    def add(name,data,state,age='Age unknown',r=None,now='2026-09-23T00:00:02Z',vehicle=None):
        out.append(dict(name=name,hex=bytes(data).hex() if data is not None else None,state=state,age=age,receipt=r or {},now=now,vehicle=vehicle))
    add('missing',None,'UNAVAILABLE');add('truncated',post[:713],'UNAVAILABLE')
    add('cleared',cleared,'INIT_AUTH');add('cleared ignores receipt',cleared,'INIT_AUTH',r=receipt)
    zero_seed=bytearray(cleared);zero_seed[48:50]=b'\0\0';add('cleared zero seed',zero_seed,'INIT_AUTH')
    badseed=bytearray(cleared);badseed[48:50]=b'\0\1';add('invalid cleared seed',badseed,'INVALID')
    add('pre auth',pre,'PRE_AUTH');add('post unknown age',post,'POST_AUTH')
    emptykey=bytearray(pre);emptykey[38:46]=bytes(8);add('zero key',emptykey,'INVALID')
    invalidvin=bytearray(pre);invalidvin[20]=ord('I');add('invalid vin',invalidvin,'INVALID')
    padded=bytearray(pre);padded[21:37]=bytes(16);add('padded structural vin',padded,'PRE_AUTH')
    add('just imported',post,'POST_AUTH','Fresh',receipt)
    add('boundary minus second',post,'POST_AUTH','Fresh',receipt,'2026-09-23T03:00:01Z')
    add('boundary exactly 3 hours',post,'POST_AUTH','Stale',receipt,'2026-09-23T03:00:02Z')
    add('one day old',post,'POST_AUTH','Stale',receipt,'2026-09-24T00:00:02Z')
    add('clock moved backwards',post,'POST_AUTH',r=receipt,now='2026-09-23T00:00:01Z')
    for key,value in [('schema','2'),('vehicle_hash','0'*64),('ssa_hash','0'*64),('stage','processed'),('imported_utc','bad'),('started_utc','bad'),('processed_utc','bad'),('pre_auth_utc','bad'),('failed_utc','bad')]:
        add('bad '+key,post,'POST_AUTH',r={**receipt,key:value})
    changed=bytearray(post);changed[100]=1;add('changed card',changed,'POST_AUTH',r=receipt)
    add('different vehicle',post,'UNAVAILABLE',r=receipt,vehicle='YS3FD49YX41000002')
    add('matching vehicle',post,'POST_AUTH','Fresh',receipt,vehicle=vin.decode())
    add('cleared different vehicle',cleared,'INIT_AUTH',r=receipt,vehicle='YS3FD49YX41000002')
    return out

class StatusTests(unittest.TestCase):
    def test_common_vectors(self):
        for c in cases():
            with self.subTest(c['name']):
                v=status.evaluate(bytes.fromhex(c['hex']) if c['hex'] is not None else None,c['receipt'],status.instant(c['now']),c['vehicle'])
                self.assertEqual((v['state'],v['age']),(c['state'],c['age']))
    def test_sparse_card_and_java_properties(self):
        c=cases()[11]
        with tempfile.TemporaryDirectory() as tmp:
            card=Path(tmp)/'card.bin';receipt=Path(tmp)/'security-processing-status.properties'
            with card.open('wb') as f:f.seek(status.OFFSET);f.write(bytes.fromhex(c['hex']))
            receipt.write_text('\n'.join(k+'='+v.replace(':','\\:') for k,v in c['receipt'].items()))
            self.assertEqual(status.read(card,receipt)['state'],'POST_AUTH')
            with patch.object(status,'datetime') as clock:
                clock.fromisoformat=datetime.fromisoformat
                clock.now.return_value=datetime(2026,9,24,tzinfo=timezone.utc)
                self.assertEqual(status.read(card,receipt)['age'],'Stale')
            receipt.write_bytes(b'\xff\xfe')
            self.assertEqual(status.read(card,receipt)['age'],'Age unknown')

if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='--export':
        Path(sys.argv[2]).write_text(json.dumps(cases()));print(f'Exported {len(cases())} shared synthetic vectors')
    elif len(sys.argv)>1 and sys.argv[1]=='--android-root':
        android=Path(sys.argv[2]);jdk=Path(os.environ.get('JAVA_HOME','/Applications/Android Studio.app/Contents/jbr/Contents/Home'))/'bin'
        if not (jdk/'javac').exists():jdk=Path(shutil.which('javac')).parent
        with tempfile.TemporaryDirectory() as tmp:
            folder=Path(tmp);inputs=folder/'vectors';inputs.mkdir();classes=folder/'classes';classes.mkdir()
            for n,c in enumerate(cases()):
                d=inputs/str(n);d.mkdir()
                props=lambda values:'\n'.join(k+'='+v.replace(':','\\:') for k,v in values.items() if v is not None)
                (d/'case.properties').write_text(props({k:c[k] for k in ('name','state','age','now','vehicle')}))
                (d/'receipt.properties').write_text(props(c['receipt']))
                if c['hex'] is not None:
                    data=bytes.fromhex(c['hex']);(d/'ssa.bin').write_bytes(data)
                    with (d/'card.bin').open('wb') as f:f.seek(status.OFFSET);f.write(data)
            sources=[android/'android/shared/com/opensaab/usb'/f for f in ('SsaData.java','SsaState.java','SecurityAccessStatus.java')]
            subprocess.run([str(jdk/'javac'),'-d',str(classes),*map(str,sources),str(Path(__file__).with_name('AndroidReference.java'))],check=True)
            subprocess.run([str(jdk/'java'),'-cp',str(classes),'AndroidReference',str(inputs)],check=True)
    else:unittest.main()
