#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""MCP stdio bridge to an explicitly enabled Android endpoint on one paired device."""
import argparse,json,os,re,socket,subprocess,sys,time

class Bridge:
    def __init__(self,serial,token_file,adb):
        if not re.fullmatch(r'[A-Za-z0-9._:-]{1,120}',serial):raise ValueError('One explicit paired ADB serial is required')
        self.serial,self.adb=serial,adb
        self.token=open(token_file,encoding='ascii').read().strip()
        if not re.fullmatch('[0-9a-f]{64}',self.token):raise ValueError('Invalid pairing code file')
        if os.stat(token_file).st_mode&0o077:raise ValueError('Pairing code file must be private (chmod 600)')
        self.port=int(subprocess.check_output([adb,'-s',serial,'forward','tcp:0','localabstract:com.opensaab.tech2.mcp'],text=True).strip())
    def close(self):subprocess.run([self.adb,'-s',self.serial,'forward','--remove','tcp:'+str(self.port)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    def request(self,op,**args):
        request=dict(args,token=self.token,op=op)
        with socket.create_connection(('127.0.0.1',self.port),timeout=4) as conn:
            conn.sendall((json.dumps(request)+'\n').encode());reply=conn.makefile('rb').readline(131073)
        if not reply or len(reply)>131072:raise RuntimeError('App endpoint unavailable or response exceeds limit')
        value=json.loads(reply)
        if 'error' in value:raise RuntimeError(value['error'])
        return value
    def state(self):return self.request('state')
    def key(self,key,state=None):
        state=state or self.state();menu=state.get('menu',{})
        encoders=dict(menu.get('key_encoders',[]))
        if key not in encoders:raise ValueError('Unknown packaged firmware key')
        if not state.get('input_idle') or not menu.get('input_idle'):raise RuntimeError('Firmware input is busy')
        return self.request('key',encoder=encoders[key],session=state['session'],revision=menu['revision'],manual_generation=state['manual_generation'])
    def entry(self,label,state):
        menu=state.get('menu',{})
        entries=[e for e in menu.get('entries',[])+menu.get('soft_keys',[]) if e.get('label')==label]
        if len(entries)==1:return entries[0]['key']
        if entries:raise RuntimeError('Ambiguous visible label')
        matches=[e for e in menu.get('list_entries',[]) if e.get('label')==label]
        selection=menu.get('selection')
        if len(matches)!=1 or not selection:raise RuntimeError('Label absent/ambiguous or selection unknown')
        return 'ENTER' if matches[0]['row']==selection['row'] else 'DOWN' if matches[0]['row']>selection['row'] else 'UP'
    def select(self,label):
        start=self.state();session=start.get('session');generation=start['manual_generation'];state=start
        end=time.monotonic()+30
        for _ in range(30):
            if state.get('session')!=session or state['manual_generation']!=generation:raise RuntimeError('Session replacement or manual takeover')
            if time.monotonic()>end:raise RuntimeError('Navigation deadline')
            menu=state.get('menu',{});key=self.entry(label,state);result=self.key(key,state)
            if not result.get('accepted'):raise RuntimeError('Key rejected')
            revision=menu.get('revision');sequence=menu.get('input_sequence');limit=time.monotonic()+5
            while time.monotonic()<limit:
                time.sleep(.05);state=self.state()
                if state.get('session')!=session or state['manual_generation']!=generation:raise RuntimeError('Session replacement or manual takeover')
                observed=state.get('menu',{})
                if state.get('input_idle') and observed.get('input_idle') and observed.get('input_sequence')!=sequence and observed.get('revision')!=revision:break
            else:raise RuntimeError('No observed firmware transition; no repeated key sent')
            if key not in ('UP','DOWN'):return {'observed_transition':True,'state':state,'workflow_completion_verified':False}
        raise RuntimeError('Navigation move limit')

def schemas():
    return [
        ('opensaab_android_get_state','Read the foreground session and original guest LCD menus.',{}),
        ('opensaab_android_get_health','Read current UI/guest liveness without interpreting a static LCD as a hang.',{}),
        ('opensaab_android_get_crash_details','Read bounded OS exit history separately from current health.',{}),
        ('opensaab_android_send_key','Send one packaged canonical key against current session/menu state.',{'key':{'type':'string'}}),
        ('opensaab_android_select_entry','Select one exact visible label, stopping on ambiguity or manual takeover.',{'label':{'type':'string'}}),
        ('opensaab_android_navigate','Follow at most twelve exact menu labels; stop at prompts and unrecognized screens.',{'labels':{'type':'array','items':{'type':'string'},'minItems':1,'maxItems':12}}),
        ('opensaab_android_start_offline','Start original firmware without a vehicle adapter.',{}),
        ('opensaab_android_stop','Stop only the currently inspected session; cleanup must be observed separately.',{'session':{'type':'string'}})]

def execute(bridge,name,args):
    allowed={n:spec for n,_,spec in schemas()}
    if name not in allowed or set(args)!=set(allowed[name]):raise ValueError('Unexpected tool or arguments')
    if name.endswith('get_state'):return bridge.state()
    if name.endswith('get_health'):return bridge.request('health')
    if name.endswith('get_crash_details'):return bridge.request('crash_details')
    if name.endswith('send_key'):return bridge.key(args['key'])
    if name.endswith('select_entry'):return bridge.select(args['label'])
    if name.endswith('navigate'):
        if not isinstance(args['labels'],list) or not 1<=len(args['labels'])<=12:raise ValueError('Bounded labels required')
        start=bridge.state();results=[]
        for label in args['labels']:
            current=bridge.state()
            if current.get('session')!=start.get('session') or current['manual_generation']!=start['manual_generation']:raise RuntimeError('Session replacement or manual takeover')
            results.append(bridge.select(label))
        return {'steps':len(results),'state':results[-1]['state'],'workflow_completion_verified':False}
    if name.endswith('start_offline'):return bridge.request('start_offline')
    if name.endswith('stop'):return bridge.request('stop',session=args['session'])
    raise ValueError('Unknown tool')

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--serial',required=True);parser.add_argument('--token-file',required=True);parser.add_argument('--adb',default=os.path.expanduser('~/Library/Android/sdk/platform-tools/adb'));parser.add_argument('--call');parser.add_argument('--arguments',default='{}');args=parser.parse_args()
    bridge=Bridge(args.serial,args.token_file,args.adb)
    try:
        if args.call:print(json.dumps(execute(bridge,args.call,json.loads(args.arguments))));return
        for line in sys.stdin:
            request={}
            try:
                if len(line)>16384:raise ValueError('Request too large')
                request=json.loads(line);method=request.get('method');ident=request.get('id')
                if ident is None:continue
                if method=='initialize':
                    requested=(request.get('params') or {}).get('protocolVersion')
                    proto=requested if requested in ('2024-11-05','2025-03-26','2025-06-18','2025-11-25') else '2025-03-26'
                    result={'protocolVersion':proto,'capabilities':{'tools':{}},'serverInfo':{'name':'opensaab-android','version':'0.1.0-preview.85'}}
                elif method=='ping':result={}
                elif method=='tools/list':result={'tools':[{'name':n,'description':d,'inputSchema':{'type':'object','properties':s,'required':list(s),'additionalProperties':False}} for n,d,s in schemas()]}
                elif method=='tools/call':
                    params=request.get('params',{});value=execute(bridge,params.get('name'),params.get('arguments',{}));result={'content':[{'type':'text','text':json.dumps(value)}]}
                else:raise ValueError('Unsupported MCP method')
                reply={'jsonrpc':'2.0','id':ident,'result':result}
            except Exception as error:reply={'jsonrpc':'2.0','id':request.get('id'),'error':{'code':-32000,'message':str(error)}}
            print(json.dumps(reply),flush=True)
    finally:bridge.close()
if __name__=='__main__':main()
