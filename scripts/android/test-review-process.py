#!/usr/bin/env python3
"""Real background process death: exact private harness report restores, consent does not.
No native firmware, adapter, upload or public app storage is used.
"""
import hashlib, io, json, re, subprocess, time, zipfile
import xml.etree.ElementTree as ET
from pathlib import Path


def exercise(adb, output, env):
    package = 'com.opensaab.beta.diagnostics'
    activity = package + '/com.opensaab.usb.SupportReportActivity'
    def run(*args, binary=False):
        return subprocess.check_output([str(x) for x in [*adb, *args]], env=env,
                                       timeout=30, text=not binary)
    def state(label):
        run('shell', 'uiautomator', 'dump', '/data/local/tmp/opensaab-review.xml')
        xml = run('shell', 'cat', '/data/local/tmp/opensaab-review.xml')
        (output / (label+'.xml')).write_text(xml)
        (output / (label+'.png')).write_bytes(run('exec-out', 'screencap', '-p', binary=True))
        return ET.fromstring(xml)
    def node(tree, text):
        matches = [n for n in tree.iter('node') if n.get('text','').casefold() == text.casefold()]
        assert len(matches) == 1, 'Expected one visible '+text
        return matches[0]
    def click(n):
        x1,y1,x2,y2 = map(int, re.findall(r'\d+', n.get('bounds')))
        assert x2>x1 and y2>y1, 'Control is not visible'
        run('shell', 'input', 'tap', str((x1+x2)//2), str((y1+y2)//2))
    def pid():
        result=subprocess.run([str(x) for x in [*adb,'shell','pidof',package]],
                              env=env,capture_output=True,text=True,timeout=10)
        return result.stdout.strip()
    run('shell','am','force-stop',package)  # Fixture isolation, not the process-loss operation.
    run('shell','run-as',package,'rm','-rf','files/support-reports')
    run('shell','run-as',package,'mkdir','-p','files/support-reports')
    name='android_support_00000000-0000-0000-0000-000000000123.zip'
    body=b'{\n  "format": 1, "description": "Synthetic process-loss review"\n}\n'
    digest=hashlib.sha256(body).hexdigest()
    archive=output/'process-loss-fixture.zip'
    with zipfile.ZipFile(archive,'w') as z:z.writestr('diagnostics.json',body)
    pointer=output/'process-loss-pointer.json'
    pointer.write_text(json.dumps({'saved_id':name,'report_sha256':digest,'review_open':True}))
    for source,dest in [(archive,name),(pointer,'review-state.json')]:
        remote='/data/local/tmp/opensaab-'+source.name
        run('push',source,remote)
        run('shell','run-as',package,'cp',remote,'files/support-reports/'+dest)
        run('shell','rm',remote)
    run('shell','am','start','-W','-n',activity)
    consent='I reviewed this report and agree to private upload'
    tree=state('process-before')
    assert node(tree,consent).get('checked')=='false'
    assert node(tree,'Send to OpenSAAB').get('enabled')=='false'
    click(node(tree,consent))
    tree=state('process-consented')
    assert node(tree,consent).get('checked')=='true'
    assert node(tree,'Send to OpenSAAB').get('enabled')=='true'
    old=pid();assert old, 'App process absent before background kill'
    run('shell','input','keyevent','KEYCODE_HOME')
    time.sleep(1)
    run('shell','am','kill',package)  # OS-style background process loss, no force-stop.
    until=time.monotonic()+10
    while pid() and time.monotonic()<until:time.sleep(.1)
    assert not pid(), 'Background process was not killed'
    run('shell','am','start','-W','-n',activity)
    new=pid();assert new and new!=old, 'New process not created'
    tree=state('process-restored')
    assert node(tree,consent).get('checked')=='false', 'Consent survived process loss'
    assert node(tree,'Send to OpenSAAB').get('enabled')=='false', 'Send enabled after process loss'
    assert any('Synthetic process-loss review' in n.get('text','') for n in tree.iter('node')), 'Different reviewed content'
    saved=run('exec-out','run-as',package,'cat','files/support-reports/'+name,binary=True)
    with zipfile.ZipFile(io.BytesIO(saved)) as z:assert z.read('diagnostics.json')==body
    info=json.loads(run('exec-out','run-as',package,'cat','files/support-reports/review-state.json'))
    assert info['saved_id']==name and info['report_sha256']==digest
    assert 'consent' not in info
    result='PASS: real HOME → am kill → relaunch in new process restores exact JSON/id/hash and an unchecked consent switch; Send disabled. No upload, adapter or firmware.\n'
    (output/'ReportProcessLoss.txt').write_text(result);print(result)
