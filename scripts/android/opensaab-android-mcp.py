#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""MCP stdio bridge to an explicitly enabled Android endpoint on one paired device."""
import argparse,json,os,re,socket,subprocess,sys,time
VERSION='0.1.1-preview.7'

class Bridge:
    def __init__(self,serial,token_file,adb):
        if not re.fullmatch(r'[A-Za-z0-9._:-]{1,120}',serial):raise ValueError('One explicit paired ADB serial is required')
        self.serial,self.adb=serial,adb
        self.request_timings=[]
        self.request_count=0;self.request_elapsed_ms=0
        with open(token_file,encoding='ascii') as token_stream:self.token=token_stream.read().strip()
        if not re.fullmatch('[0-9a-f]{64}',self.token):raise ValueError('Invalid pairing code file')
        if os.stat(token_file).st_mode&0o077:raise ValueError('Pairing code file must be private (chmod 600)')
        self.port=int(subprocess.check_output([adb,'-s',serial,'forward','tcp:0','localabstract:com.opensaab.tech2.mcp'],text=True).strip())
    def close(self):subprocess.run([self.adb,'-s',self.serial,'forward','--remove','tcp:'+str(self.port)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    def request(self,op,**args):
        request=dict(args,token=self.token,op=op)
        began=time.monotonic()
        try:
            with socket.create_connection(('127.0.0.1',self.port),timeout=4) as conn:
                conn.sendall((json.dumps(request)+'\n').encode());reply=conn.makefile('rb').readline(131073)
            if not reply or len(reply)>131072:raise RuntimeError('App endpoint unavailable or response exceeds limit')
            value=json.loads(reply)
            if 'error' in value:raise RuntimeError(value['error'])
            return value
        finally:
            elapsed=(time.monotonic()-began)*1000
            self.request_count+=1;self.request_elapsed_ms+=elapsed
            self.request_timings.append((op,elapsed))
            # Metrics contain no arguments, tokens, vehicle data or menu text.
            if len(self.request_timings)>1024:del self.request_timings[:512]
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
    @staticmethod
    def check_owner(state,session,generation):
        if state.get('session')!=session or state['manual_generation']!=generation:raise RuntimeError('Session replacement or manual takeover')
    def wait_transition(self,before,timeout=5):
        menu=before.get('menu',{});limit=time.monotonic()+timeout
        while time.monotonic()<limit:
            if 'wait_state_v1' in before.get('mcp_features',[]):
                reply=self.request('wait_state',session=before['session'],manual_generation=before['manual_generation'],after_sequence=menu['input_sequence'],after_revision=menu['revision'],wait_ms=max(1,min(1000,int((limit-time.monotonic())*1000))))
                state=reply['state']
            else:
                time.sleep(min(.05,max(0,limit-time.monotonic())));state=self.state()
            self.check_owner(state,before.get('session'),before['manual_generation'])
            observed=state.get('menu',{})
            if state.get('input_idle') and observed.get('input_idle') and observed.get('input_sequence',-1)>menu.get('input_sequence',-1) and observed.get('revision')!=menu.get('revision'):return state
        raise RuntimeError('No observed firmware transition; no repeated key sent')
    def select(self,label,state=None):
        if not isinstance(label,str) or not label.strip():raise ValueError('One exact nonempty label required')
        start=state if state is not None else self.state();session=start.get('session');generation=start['manual_generation'];state=start
        end=time.monotonic()+30
        for _ in range(30):
            self.check_owner(state,session,generation)
            if time.monotonic()>end:raise RuntimeError('Navigation deadline')
            menu=state.get('menu',{});key=self.entry(label,state);result=self.key(key,state)
            if not result.get('accepted'):raise RuntimeError('Key rejected')
            state=self.wait_transition(state,min(5,max(0,end-time.monotonic())))
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
    began=time.monotonic();count=bridge.request_count;request_ms=bridge.request_elapsed_ms
    value=_execute(bridge,name,args)
    value['connector_timing']={'elapsed_ms':round((time.monotonic()-began)*1000,2),'requests':bridge.request_count-count,'request_ms':round(bridge.request_elapsed_ms-request_ms,2)}
    return value

def _execute(bridge,name,args):
    allowed={n:spec for n,_,spec in schemas()}
    if name not in allowed or set(args)!=set(allowed[name]):raise ValueError('Unexpected tool or arguments')
    if name.endswith('get_state'):return bridge.state()
    if name.endswith('get_health'):return bridge.request('health')
    if name.endswith('get_crash_details'):return bridge.request('crash_details')
    if name.endswith('send_key'):return bridge.key(args['key'])
    if name.endswith('select_entry'):return bridge.select(args['label'])
    if name.endswith('navigate'):
        if not isinstance(args['labels'],list) or not 1<=len(args['labels'])<=12 or any(not isinstance(label,str) or not label.strip() for label in args['labels']):raise ValueError('Bounded exact labels required')
        start=bridge.state();state=start;results=[]
        for label in args['labels']:
            bridge.check_owner(state,start.get('session'),start['manual_generation'])
            result=bridge.select(label,state);state=result['state'];results.append(result)
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
                    result={'protocolVersion':proto,'capabilities':{'tools':{}},'serverInfo':{'name':'opensaab-android','version':VERSION}}
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
