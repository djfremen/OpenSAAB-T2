"""Use through an SSH local forward. Token is a private file, never a CLI argument."""
import argparse
import base64
import json
import http.client
import sys
from pathlib import Path
import urllib.request
import urllib.error

class Client:
    def __init__(self, token_file, url='http://127.0.0.1:18765'):
        self.token=Path(token_file).read_text().strip(); self.url=url
    def call(self, route, data=None):
        req=urllib.request.Request(self.url+route, data=json.dumps(data or {}).encode(),
            headers={'Authorization':'Bearer '+self.token,'Content-Type':'application/json'})
        try:
            with urllib.request.urlopen(req, timeout=15) as r: return json.load(r)
        except urllib.error.HTTPError as e:
            raise RuntimeError(f'HTTP {e.code}: {e.read().decode()}') from None
        except (OSError, http.client.HTTPException) as e:
            raise RuntimeError('Harness connection unavailable; check SSH forward and Windows scheduled task') from e

def main():
    p=argparse.ArgumentParser(); p.add_argument('route'); p.add_argument('json',nargs='?',default='{}')
    p.add_argument('--token-file',type=Path,required=True); p.add_argument('--url',default='http://127.0.0.1:18765'); p.add_argument('--png',type=Path)
    a=p.parse_args(); result=Client(a.token_file,a.url).call(a.route,json.loads(a.json))
    screenshot=result.get('screenshot',result)
    if 'png' in screenshot:
        data=base64.b64decode(screenshot.pop('png'))
        if a.png:
            a.png.write_bytes(data); screenshot['saved_png']=str(a.png)
    print(json.dumps(result,indent=2))
if __name__=='__main__':
    try: main()
    except RuntimeError as e: print(str(e),file=sys.stderr);sys.exit(1)
