# SPDX-License-Identifier: MPL-2.0
"""Small private support inbox using the collector's existing S3/R2 client."""
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
FIELDS.update('''device_resources native_performance performance_schema sample_interval_ms sample_count
first_frame_observed_ms complete samples cpu_ms peak_rss_kib minor_faults major_faults
ram_total_kib ram_available_kib swap_free_kib frame_age_ms ram_total_bytes ram_available_bytes
low_memory_threshold_bytes system_low_memory app_heap_used_bytes app_heap_limit_bytes
app_native_heap_bytes storage_free_bytes display_width_px display_height_px density_dpi runtime_cpu_count'''.split())

def validate(value, depth=0):
    if depth > 8:
        raise ValueError('Report is too deeply nested')
    if isinstance(value, dict):
        if len(value) > 48 or any(k not in FIELDS for k in value):
            raise ValueError('Unsupported report fields')
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
