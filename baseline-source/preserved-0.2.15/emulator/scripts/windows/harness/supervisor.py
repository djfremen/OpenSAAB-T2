"""Interactive-session supervisor. Exit 0 is intentional; other exits restart."""
import argparse
import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path
import subprocess
import sys
import time

p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);a=p.parse_args()
log=logging.getLogger('supervisor');log.setLevel(logging.INFO)
h=RotatingFileHandler(a.root/'supervisor.log',maxBytes=1_000_000,backupCount=3)
h.setFormatter(logging.Formatter('%(asctime)s %(message)s'));log.addHandler(h)
delay=2
while True:
    err=a.root/'agent-errors.log'
    if err.exists() and err.stat().st_size>1_000_000:
        err.replace(a.root/'agent-errors.previous.log')
    started=time.monotonic()
    with err.open('ab',buffering=0) as output:
        child=subprocess.Popen([sys.executable,str(a.root/'agent.py'),'--root',str(a.root)],stdout=output,stderr=subprocess.STDOUT)
        log.info('Started agent pid=%s',child.pid)
        code=child.wait()
    log.info('Agent exited code=%s runtime=%.1fs',code,time.monotonic()-started)
    if code==0:break
    if time.monotonic()-started>300:delay=2
    log.info('Restarting in %ss',delay);time.sleep(delay);delay=min(delay*2,30)
