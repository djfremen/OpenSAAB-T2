# SPDX-License-Identifier: MPL-2.0
"""Small private support inbox using the collector's existing S3/R2 client."""
import datetime
import hashlib
import json
import re
import secrets
import threading
import time
from collections import deque
from fastapi import APIRouter, HTTPException, Request
from starlette.concurrency import run_in_threadpool

MAX_BYTES = 65536
# Only fields produced by the reviewed Android support report. No raw logs or files.
FIELDS = set('''format created_utc description build_profile apk_abi app version version_code
android_api device_model abis privacy recent_sessions adapter modified_utc logs source
file_bytes tail_only event_counts USB_OPEN USB_CLOSED USB_TX USB_RX CAN_TX CAN_RX
TIMEOUT DISCONNECT PANIC ERROR FAILED emulator_outcome exit_code
instructions pc host_cancelled_operations status connection_attempts stage outcome reason
elapsed_ms stages usb_vendor_id usb_product_id attempt_id android_process_exits timestamp_ms
pss_kib rss_kib last-app-crash.json last-app-error.json utc kind exception_class stack unavailable'''.split())
FIELDS.add('NEGATIVE RESPONSE')
FIELDS.update('emulator_health heartbeat_age_ms input_wait_ms ui_delay_ms'.split())
FIELDS.update('''security_processing security_reset started_utc processed_utc imported_utc
failed_utc request_id provider vehicle_access_verified cleared_utc'''.split())
FIELDS.update('''device_resources native_performance performance_schema sample_interval_ms sample_count
first_frame_observed_ms complete samples cpu_ms peak_rss_kib minor_faults major_faults
ram_total_kib ram_available_kib swap_free_kib frame_age_ms ram_total_bytes ram_available_bytes
low_memory_threshold_bytes system_low_memory app_heap_used_bytes app_heap_limit_bytes
app_native_heap_bytes storage_free_bytes display_width_px display_height_px density_dpi runtime_cpu_count'''.split())

# Reviewed Android diagnostic candidate: fixed labels and bounded lifecycle evidence.
FIELDS.update('''firmware_version session_id diagnostic_events event failure_category failure_categories
category tail_line incident_relation orientation_start orientation_end pause_count resume_count'''.split())
FIELDS.update('security_status auth_status freshness observed_utc status_utc'.split())
DIAGNOSTIC_EVENTS = set('START EXPECTED_STOP UNEXPECTED_EXIT PAUSED RESUMED SECURITY_COLLECTION_REQUESTED SECURITY_PROCESS_REQUESTED SECURITY_IMPORTED RESTART_REQUESTED'.split())
FAILURE_CATEGORIES = set('unclassified adapter_firmware_not_validated native_mode_mismatch usb_layout_not_validated usb_open_denied usb_interface_claim_failed usb_startup_drain_failed local_bridge_auth_failed usb_write_incomplete usb_read_detached local_bridge_disconnected command_policy_rejected firmware_missing adapter_status_error adapter_deadline adapter_reopen_required session_time_limit adapter_cleanup_incomplete adapter_startup_failed'.split())

# Bounded offline-emulation evidence v1; labels only, never source reasons or screens.
FIELDS.update("""emulation_evidence emulation_context schema candi attempted completed cycles
serial_rx_bytes serial_rx_breaks serial_tx_bytes adapter_tx_confirmations native_uart external_tx
access address width profile engine_sha256 firmware_sha256 eprom.bin opsys.dwn candi.bin card.bin instruction_limit
deadline_ms session_end guest_crash input_count frame_count last_input_elapsed_ms last_frame_elapsed_ms
forced_stop submitter_notes_provided retained_sample_count samples_truncated max_frame_age_ms min_ram_available_kib""".split())
STOP_CODES = {'output_failure','adapter_failure','candi_stopped','instruction_budget','operator_or_deadline','operator_stop','guest_bootstrap_failure','guest_cpu_fault','link_unavailable','completed','unknown'}
SCREEN_STAGES = {'link_unavailable','firmware_missing','vehicle_link_wait','main_menu','model_year','dtc_menu','diagnostics_menu','other_unknown'}
CANDI_REASONS = {'unsupported_access','cpu_fault','instruction_budget','host_cancelled','none','unknown'}
CANDI_NUMBERS = {'attempted','completed','cycles','pc','serial_rx_bytes','serial_rx_breaks','serial_tx_bytes','adapter_tx_confirmations','address','width'}
END_REASONS = {'background_stop','user_stop','startup_failure','host_deadline','forced_stop','process_exit','exited','adapter_stopped','wake_rejected','unsupported_command','app_quit'}
END_NUMBERS = {'elapsed_ms','input_count','frame_count','last_input_elapsed_ms','last_frame_elapsed_ms'}
def evidence_object(v, allowed, labels=None, numbers=(), booleans=()):
    if not isinstance(v, dict) or set(v) - set(allowed):
        raise ValueError('Unsupported emulation evidence fields')
    if v.get('unavailable') is True:
        if set(v) - {'unavailable','schema','session_id'}:
            raise ValueError('Invalid unavailable evidence')
    elif 'unavailable' in v:
        raise ValueError('Invalid unavailable evidence')
    for key, choices in (labels or {}).items():
        if key in v and (not isinstance(v[key],str) or v[key] not in choices):
            raise ValueError('Unsupported emulation evidence label')
    for key in numbers:
        if key in v and (type(v[key]) is not int or v[key] < 0):
            raise ValueError('Invalid emulation evidence number')
    for key in booleans:
        if key in v and type(v[key]) is not bool:
            raise ValueError('Invalid emulation evidence flag')

def validate_emulation(v):
    for key in ('submitter_notes_provided','samples_truncated','guest_crash'):
        if key in v and type(v[key]) is not bool:
            raise ValueError('Invalid report flag')
    if 'emulation_evidence' in v:
        e=v['emulation_evidence']
        evidence_object(e, {'schema','reason','stage','candi','unavailable'}, {'reason':STOP_CODES,'stage':SCREEN_STAGES})
        if not e.get('unavailable') and (type(e.get('schema')) is not int or e['schema'] != 1 or 'reason' not in e or 'stage' not in e):
            raise ValueError('Invalid emulation evidence schema')
        if 'candi' in e:
            evidence_object(e['candi'], CANDI_NUMBERS|{'reason','native_uart','external_tx','access'}, {'reason':CANDI_REASONS,'access':{'read','write'}}, CANDI_NUMBERS, {'native_uart','external_tx'})
    if 'emulation_context' in v:
        e=v['emulation_context']
        evidence_object(e, {'schema','session_id','profile','started_utc','engine_sha256','firmware_sha256','instruction_limit','deadline_ms','unavailable'}, {'profile':{'offline_research'}}, {'schema','instruction_limit','deadline_ms'})
        if not e.get('unavailable') and (e.get('schema') != 1 or e.get('profile') != 'offline_research'):
            raise ValueError('Invalid emulation context schema')
        if 'engine_sha256' in e and (not isinstance(e['engine_sha256'],str) or not re.fullmatch('[a-f0-9]{64}',e['engine_sha256'])):
            raise ValueError('Invalid executable fingerprint')
        if 'firmware_sha256' in e:
            hashes=e['firmware_sha256']
            if not isinstance(hashes,dict) or set(hashes)-{'eprom.bin','opsys.dwn','candi.bin','card.bin'} or any(not isinstance(h,str) or not re.fullmatch('[a-f0-9]{64}',h) for h in hashes.values()):
                raise ValueError('Invalid firmware fingerprints')
        if 'started_utc' in e:
            try:
                if datetime.datetime.fromisoformat(e['started_utc'].replace('Z','+00:00')).tzinfo is None: raise ValueError()
            except (ValueError,TypeError,AttributeError): raise ValueError('Invalid emulation timestamp')
    if 'session_end' in v:
        e=v['session_end']
        evidence_object(e, END_NUMBERS|{'reason','exit_code','forced_stop','unavailable'}, {'reason':END_REASONS}, END_NUMBERS, {'forced_stop'})
        if 'exit_code' in e and type(e['exit_code']) is not int: raise ValueError('Invalid process exit code')

def validate(value, depth=0):
    if depth > 8:
        raise ValueError('Report is too deeply nested')
    if isinstance(value, dict):
        validate_emulation(value)
        if len(value) > 48 or any(k not in FIELDS for k in value):
            raise ValueError('Unsupported report fields')
        if 'security_status' in value:
            auth = value['security_status']
            if (not isinstance(auth, dict) or set(auth) - {'auth_status', 'freshness', 'observed_utc', 'status_utc', 'vehicle_access_verified'}
                    or auth.get('auth_status') not in ('[INIT_AUTH]', '[PRE-AUTH]', '[POST-AUTH]', '[INVALID]', '[N/A]')
                    or auth.get('freshness') not in ('Fresh', 'Stale', 'Age unknown')
                    or auth.get('vehicle_access_verified') is not False or 'observed_utc' not in auth):
                raise ValueError('Invalid security status snapshot')
            for key in ('observed_utc', 'status_utc'):
                if key in auth:
                    stamp = auth[key]
                    if not isinstance(stamp, str) or len(stamp) > 40:
                        raise ValueError('Invalid status timestamp')
                    try:
                        parsed = datetime.datetime.fromisoformat(stamp.replace('Z', '+00:00'))
                        if parsed.tzinfo is None:
                            raise ValueError('Timezone required')
                    except (ValueError, TypeError):
                        raise ValueError('Invalid status timestamp')
        if 'firmware_version' in value and (not isinstance(value['firmware_version'], str) or
                not re.fullmatch(r'[0-9]{1,3}(?:\.[0-9]{1,3}){1,3}(?:[-+][A-Za-z0-9._-]{1,20})?', value['firmware_version'])):
            raise ValueError('Unsupported firmware version')
        for key, allowed in (('event', DIAGNOSTIC_EVENTS), ('category', FAILURE_CATEGORIES),
                             ('failure_category', FAILURE_CATEGORIES),
                             ('incident_relation', {'unknown', 'same_session', 'different_session'})):
            if key in value and (not isinstance(value[key], str) or value[key] not in allowed):
                raise ValueError('Unsupported diagnostic label')
        if 'session_id' in value and (not isinstance(value['session_id'], str) or
                not re.fullmatch(r'(?:chipsoft|native|offline)-[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}', value['session_id'])):
            raise ValueError('Unsupported session identifier')
        for key, limit in (('diagnostic_events', 32), ('failure_categories', 16)):
            if key in value and (not isinstance(value[key], list) or len(value[key]) > limit):
                raise ValueError('Too many diagnostic entries')
        for child in value.values():
            validate(child, depth + 1)
    elif isinstance(value, list):
        if len(value) > 40:
            raise ValueError('Too many report entries')
        for child in value:
            validate(child, depth + 1)
    elif isinstance(value, str):
        if len(value) > 2048:
            raise ValueError('Report text is too long')
    elif value is not None and not isinstance(value, (bool, int)):
        raise ValueError('Unsupported report value')

def parse_report(body):
    if not body or len(body) > MAX_BYTES:
        raise ValueError('Report size is invalid')
    report = json.loads(body.decode('utf-8'))
    validate(report)
    if not isinstance(report, dict) or report.get('format') != 1:
        raise ValueError('Unsupported report format')
    if report.get('app') not in ('com.opensaab.tech2', 'com.opensaab.tech2.headunit32'):
        raise ValueError('Unsupported application')
    if not isinstance(report.get('description'), str) or not isinstance(report.get('android_api'), int):
        raise ValueError('Required report information is missing')
    return json.dumps(report, ensure_ascii=True, sort_keys=True, separators=(',', ':')).encode()

class Limits:
    def __init__(self):
        self.lock = threading.Lock()
        self.clients = {}
        self.total = deque()
    def accept(self, address):
        now = time.monotonic()
        with self.lock:
            while self.total and self.total[0] < now - 3600:
                self.total.popleft()
            self.clients = {k: v for k, v in self.clients.items() if v and v[-1] >= now - 3600}
            q = self.clients.setdefault(address, deque())
            while q and q[0] < now - 3600:
                q.popleft()
            if len(q) >= 10 or len(self.total) >= 100:
                raise HTTPException(429, 'Please try sending later', headers={'Retry-After': '3600'})
            q.append(now)
            self.total.append(now)

def make_router(storage, bucket, admin_token):
    router = APIRouter()
    limits = Limits()

    def configured():
        client = storage()
        if client is None or not bucket() or not admin_token():
            raise HTTPException(503, 'Report storage is temporarily unavailable')
        return client

    def save(body):
        client = configured()
        digest = hashlib.sha256(body).hexdigest()
        report_id = 'OS-' + digest[:24]
        key = 'support-reports/v1/' + report_id + '.json'
        # Content-addressed key: retrying the same frozen report keeps its number.
        try:
            client.put_object(Bucket=bucket(), Key=key, Body=body,
                              ContentType='application/json', Metadata={'sha256': digest})
            stored = client.head_object(Bucket=bucket(), Key=key)
            if stored.get('ContentLength') != len(body) or stored.get('Metadata', {}).get('sha256') != digest:
                raise RuntimeError('Storage verification failed')
        except Exception:
            raise HTTPException(503, 'Report was not confirmed; keep your local copy and retry') from None
        return {'report_id': report_id, 'stored': True}

    @router.post('/api/support/reports', status_code=201)
    async def upload(request: Request):
        if request.headers.get('X-OpenSAAB-Consent') != 'support-report-v1':
            raise HTTPException(400, 'Review and confirm the report before sending')
        if request.headers.get('content-type', '').split(';')[0].strip() != 'application/json':
            raise HTTPException(415, 'Expected a JSON report')
        # Edge-provided addresses are intentionally not trusted as authentication.
        # Limits are process-local and conservative; no address is stored in R2.
        limits.accept(request.client.host if request.client else 'unknown')
        body = bytearray()
        async for chunk in request.stream():
            body.extend(chunk)
            if len(body) > MAX_BYTES:
                raise HTTPException(413, 'Report is too large')
        try:
            clean = parse_report(bytes(body))
        except (ValueError, TypeError, RecursionError):
            raise HTTPException(400, 'Invalid support report') from None
        if len(clean) > MAX_BYTES:
            raise HTTPException(413, 'Report is too large')
        return await run_in_threadpool(save, clean)

    @router.get('/api/admin/support/reports/{report_id}')
    def retrieve(report_id: str, request: Request):
        expected = admin_token()
        supplied = request.headers.get('Authorization', '')
        if not expected or not secrets.compare_digest(supplied, 'Bearer ' + expected):
            raise HTTPException(401, 'Administrator access required')
        if not re.fullmatch(r'OS-[a-f0-9]{24}', report_id):
            raise HTTPException(404, 'Report not found')
        try:
            obj = configured().get_object(Bucket=bucket(), Key='support-reports/v1/' + report_id + '.json')
            body = obj['Body'].read(MAX_BYTES + 1)
            if len(body) > MAX_BYTES:
                raise ValueError('Oversized stored object')
            return json.loads(body)
        except HTTPException:
            raise
        except Exception:
            raise HTTPException(404, 'Report unavailable') from None
    return router
