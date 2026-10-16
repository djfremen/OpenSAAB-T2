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

# Desktop shells (macOS/Windows/Linux) share one Rust collector with the same privacy rules.
FIELDS.update('''platform arch build os_version startup native_boot_ms session_end guest_crash
message'''.split())
ANDROID_APPS = ('com.opensaab.tech2', 'com.opensaab.tech2.headunit32')
DESKTOP_APP = 'com.opensaab.desktop'
IOS_APP = 'com.opensaab.t2.ios.preview'
FIELDS.update('model failure_stage'.split())
# Where a native Rust panic happened. Its inner keys are checked here, not added to FIELDS,
# so "file"/"line" stay invalid everywhere else. The panic message is never accepted.
FIELDS.add('panic_location')
PANIC_FILE = re.compile(r'[A-Za-z0-9_.+-]{1,80}(?:/[A-Za-z0-9_.+-]{1,80}){0,2}')

def validate_panic_location(v):
    if (not isinstance(v, dict) or set(v) != {'file', 'line', 'column', 'thread'}
            or not isinstance(v['file'], str) or not PANIC_FILE.fullmatch(v['file'])
            or any(part in ('.', '..') for part in v['file'].split('/'))
            or any(type(v[k]) is not int or not 0 <= v[k] <= 999999 for k in ('line', 'column'))
            or not isinstance(v['thread'], str) or not re.fullmatch(r'[A-Za-z0-9_.:<> -]{0,64}', v['thread'])):
        raise ValueError('Invalid panic location')
IOS_FAILURE_STAGES = {'unexpected_emulator_exit','emulator_heartbeat_missing','input_without_display_response','ui_unresponsive','session_interrupted'}

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
EMULATION_PROFILES = {'offline_research', 'offline_native', 'mxp_research'}
CANDI_NUMBERS = {'attempted','completed','cycles','pc','serial_rx_bytes','serial_rx_breaks','serial_tx_bytes','adapter_tx_confirmations','address','width'}
END_REASONS = {'background_stop','user_stop','normal_stop','startup_failure','host_deadline','forced_stop','process_exit','exited','adapter_stopped','wake_rejected','unsupported_command','app_quit'}
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
        evidence_object(e, {'schema','session_id','profile','started_utc','engine_sha256','firmware_sha256','instruction_limit','deadline_ms','unavailable'}, {'profile':EMULATION_PROFILES}, {'schema','instruction_limit','deadline_ms'})
        if not e.get('unavailable') and (e.get('schema') != 1 or e.get('profile') not in EMULATION_PROFILES):
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


# Cross-platform diagnostic evidence v2. No raw strings or memory payloads.
FIELDS.update("diagnostic_evidence artifacts terminal_fault recent_pcs session_mode last_operation primary_reason cleanup child_exit_code forced_termination usb_release protocol_cleanup startup_result startup_complete query_record_verified installed_record_verified channel_cleanup usb_closed fresh_dh_exchanges vehicle_tx report.json crash.json session-end.json host-performance.json connection-evidence.json cleanup-evidence.json result.json".split())
EVIDENCE_ARTIFACTS={'report.json','crash.json','session-end.json','host-performance.json','connection-evidence.json','cleanup-evidence.json','result.json'}
EVIDENCE_ADAPTERS={'chipsoft','nano','mdi','mongoose','obdlink','vlinker','offline','unknown'}
def validate_diagnostic(e):
    if not isinstance(e,dict) or set(e)-{'schema','adapter','artifacts','attempt_id','session_mode','last_operation','primary_reason','cleanup','terminal_fault','startup_result'} or type(e.get('schema')) is not int or e['schema']!=2 or e.get('adapter') not in EVIDENCE_ADAPTERS:
        raise ValueError('Invalid diagnostic evidence')
    artifacts=e.get('artifacts')
    if not isinstance(artifacts,dict) or set(artifacts)-EVIDENCE_ARTIFACTS or any(v not in ('available','missing','unreadable','oversize','invalid') for v in artifacts.values()):raise ValueError('Invalid artifact availability')
    if 'attempt_id' in e and (not isinstance(e['attempt_id'],str) or not re.fullmatch(r'[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}',e['attempt_id'])):raise ValueError('Invalid diagnostic attempt')
    for key,allowed in {'session_mode':{'offline','read_only','full_native','security_seeds','unknown'},'last_operation':{'connection','vin','dtc_read','dtc_clear','ecu_info','engine_data','security','original_menus','manual_input','unknown'},'primary_reason':{'NONE','NO_ADAPTER','MULTIPLE_ADAPTERS','PERMISSION_DENIED','DISCONNECTED','TIMEOUT','IO_ERROR','PROTOCOL_OR_PROCESS_ERROR','VIN_UNAVAILABLE','CLEANUP_FAILED','USER_STOP','SETUP_REQUIRED'}}.items():
        if key in e and (not isinstance(e[key],str) or e[key] not in allowed):raise ValueError('Invalid diagnostic label')
    if 'terminal_fault' in e:
        f=e['terminal_fault']
        if not isinstance(f,dict) or set(f)-{'pc','recent_pcs'}:raise ValueError('Invalid fault evidence')
        pcs=f.get('recent_pcs',[])
        if not isinstance(pcs,list) or len(pcs)>32 or any(type(v) is not int or not 0<=v<=4294967295 for v in pcs):raise ValueError('Invalid execution history')
        if 'pc' in f and f['pc'] is not None and (type(f['pc']) is not int or not 0<=f['pc']<=4294967295):raise ValueError('Invalid fault address')
    if 'cleanup' in e:
        c=e['cleanup']
        if not isinstance(c,dict) or set(c)-{'child_exit_code','forced_termination','usb_release','protocol_cleanup'}:raise ValueError('Invalid cleanup evidence')
        if 'child_exit_code' in c and (type(c['child_exit_code']) is not int or not -2147483648<=c['child_exit_code']<=2147483647):raise ValueError('Invalid child exit')
        if 'forced_termination' in c and type(c['forced_termination']) is not bool:raise ValueError('Invalid forced termination')
        for k in ('usb_release','protocol_cleanup'):
            if k in c and c[k] not in ('succeeded','failed','unknown','not_attempted'):raise ValueError('Invalid cleanup status')
    if 'startup_result' in e:
        r=e['startup_result'];flags={'startup_complete','query_record_verified','installed_record_verified','channel_cleanup','usb_closed'};nums={'fresh_dh_exchanges','vehicle_tx'}
        if not isinstance(r,dict) or set(r)-flags-nums:raise ValueError('Invalid startup result')
        if any(type(r[k]) is not bool for k in flags&r.keys()) or any(type(r[k]) is not int or r[k]<0 for k in nums&r.keys()):raise ValueError('Invalid startup values')

# Build/program identity, schema 1. Three separate axes, never merged:
#   program_identity  the native engine that ran a session (written when the session is created)
#   shell_identity    the app that launched that session (written before the engine starts)
#   preparer_identity the app that froze this report for review
# Send-time uploader identity is NOT part of the report body: it arrives in a header and is
# stored in a separate submission-event object, so the frozen bytes and receipt never change.
# Every identity here is a client claim until its executable/package hash matches a trusted manifest.
#   firmware_identity the diagnostic card/boot/opsys/CANdi images and adapter firmware a session
#                     actually loaded (hashes of loaded bytes, never a selected UI label)
IDENTITY_CONTAINERS = {'program_identity', 'shell_identity', 'preparer_identity', 'firmware_identity'}
IDENTITY_ONLY = set('''identity_schema name architecture source_revision source_dirty channel build_epoch
executable_sha256 executable_present package package_sha256 recorded_by engine_version
card_sha256 eprom_sha256 opsys_sha256 candi_sha256 card_id card_bytes adapter_kind adapter_firmware
adapter_hardware'''.split())
FIRMWARE_HASHES = ('card_sha256', 'eprom_sha256', 'opsys_sha256', 'candi_sha256')
FIELDS.update(IDENTITY_CONTAINERS | IDENTITY_ONLY)
IDENTITY_ARCHES = {'arm64', 'x86_64', 'arm', 'armv7', 'x86', 'unknown'}
IDENTITY_CHANNELS = {'release', 'preview', 'dev', 'unknown'}
IDENTITY_RECORDERS = {'core', 'shell_prelaunch', 'shell'}
IDENTITY_PLATFORMS = {'android', 'macos', 'windows', 'linux', 'ios'}
IDENTITY_PACKAGES = set(ANDROID_APPS) | {DESKTOP_APP, IOS_APP}
# Digit-led release versions (0.1-dev, 0.1.0-preview.90, 0.2.25-recovered-core.4) or explicit placeholders.
_VERSION_RE = re.compile(r'(?=.{1,64}$)(?:\d{1,3}(?:\.\d{1,3}){1,3}(?:[-+][0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?|development|unknown)')
_CARD_ID_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,79}')
FIRMWARE_ROLES = {'card_sha256': 'card', 'eprom_sha256': 'eprom', 'opsys_sha256': 'opsys', 'candi_sha256': 'candi'}
_NAME_RE = re.compile(r'[A-Za-z0-9][A-Za-z0-9 ._()-]{0,63}')
_REVISION_RE = re.compile(r'[a-f0-9]{40}|unknown')
_SHA256_RE = re.compile(r'[a-f0-9]{64}')
UPLOADER_HEADER = 'X-OpenSAAB-Uploader'
UPLOADER_MAX_BYTES = 1024

def _identity_common(v, allowed, required):
    if not isinstance(v, dict) or set(v) - allowed or not required <= set(v):
        raise ValueError('Invalid build identity')
    if v.get('identity_schema') != 1 or type(v['identity_schema']) is not int:
        raise ValueError('Invalid build identity schema')
    if not isinstance(v.get('version'), str) or not _VERSION_RE.fullmatch(v['version']):
        raise ValueError('Invalid build version')
    if not isinstance(v.get('source_revision'), str) or not _REVISION_RE.fullmatch(v['source_revision']):
        raise ValueError('Invalid source revision')
    if v.get('channel') not in IDENTITY_CHANNELS:
        raise ValueError('Invalid build channel')
    for key in ('source_dirty', 'executable_present'):
        if key in v and type(v[key]) is not bool:
            raise ValueError('Invalid build identity flag')
    for key in ('build_epoch', 'version_code'):
        if key in v and (type(v[key]) is not int or not 0 <= v[key] <= 2**53):
            raise ValueError('Invalid build identity number')
    for key in ('executable_sha256', 'package_sha256'):
        if key in v and (not isinstance(v[key], str) or not _SHA256_RE.fullmatch(v[key])):
            raise ValueError('Invalid build fingerprint')
    if 'architecture' in v and v['architecture'] not in IDENTITY_ARCHES:
        raise ValueError('Invalid build architecture')
    if 'recorded_by' in v and v['recorded_by'] not in IDENTITY_RECORDERS:
        raise ValueError('Invalid identity recorder')
    if 'platform' in v and v['platform'] not in IDENTITY_PLATFORMS:
        raise ValueError('Invalid identity platform')

def validate_program_identity(v):
    """Engine identity. Accepts the existing canonical 3-field form (legacy, unverifiable) or schema 1."""
    if isinstance(v, dict) and 'identity_schema' not in v:
        if (set(v) - {'name', 'version', 'architecture'} or not isinstance(v.get('version'), str)
                or not _VERSION_RE.fullmatch(v['version'])
                or ('name' in v and (not isinstance(v['name'], str) or not _NAME_RE.fullmatch(v['name'])))
                or ('architecture' in v and v['architecture'] not in IDENTITY_ARCHES)):
            raise ValueError('Invalid legacy program identity')
        return
    _identity_common(v, {'identity_schema', 'name', 'version', 'architecture', 'source_revision', 'source_dirty',
                         'channel', 'build_epoch', 'executable_sha256', 'executable_present', 'recorded_by'},
                     {'identity_schema', 'name', 'version', 'architecture', 'source_revision', 'channel'})
    if not isinstance(v['name'], str) or not _NAME_RE.fullmatch(v['name']):
        raise ValueError('Invalid program name')
    if v.get('executable_present') is False and 'executable_sha256' in v:
        raise ValueError('Missing executable cannot have a fingerprint')

def validate_app_identity(v):
    """Shell or preparer identity: the installed app package, not the engine crate version."""
    _identity_common(v, {'identity_schema', 'package', 'version', 'version_code', 'source_revision', 'source_dirty',
                         'channel', 'build_epoch', 'package_sha256', 'architecture', 'platform', 'recorded_by',
                         'executable_sha256', 'executable_present', 'engine_version'},
                     {'identity_schema', 'package', 'version', 'source_revision', 'channel'})
    if v['package'] not in IDENTITY_PACKAGES:
        raise ValueError('Invalid app package')
    if v.get('executable_present') is False and 'executable_sha256' in v:
        raise ValueError('Missing executable cannot have a fingerprint')
    if 'engine_version' in v and (not isinstance(v['engine_version'], str) or not _VERSION_RE.fullmatch(v['engine_version'])):
        raise ValueError('Invalid engine version')

def validate_firmware_identity(v):
    """Images and adapter firmware actually loaded for a session; hashes are of loaded bytes."""
    allowed = {'identity_schema', 'recorded_by', 'card_id', 'card_bytes', 'adapter_kind', 'adapter_firmware',
               'adapter_hardware', *FIRMWARE_HASHES}
    if not isinstance(v, dict) or set(v) - allowed or v.get('identity_schema') != 1 or type(v['identity_schema']) is not int:
        raise ValueError('Invalid firmware identity')
    for key in FIRMWARE_HASHES:
        if key in v and (not isinstance(v[key], str) or not _SHA256_RE.fullmatch(v[key])):
            raise ValueError('Invalid firmware fingerprint')
    if 'card_id' in v and (not isinstance(v['card_id'], str) or not _CARD_ID_RE.fullmatch(v['card_id'])):
        raise ValueError('Invalid card identifier')
    if 'card_bytes' in v and (type(v['card_bytes']) is not int or not 0 < v['card_bytes'] <= 2**31):
        raise ValueError('Invalid card size')
    if 'adapter_kind' in v and v['adapter_kind'] not in EVIDENCE_ADAPTERS:
        raise ValueError('Invalid adapter kind')
    if 'adapter_firmware' in v and (not isinstance(v['adapter_firmware'], str) or
            not re.fullmatch(r'[0-9]{1,3}(?:\.[0-9]{1,3}){1,3}(?:[-+][A-Za-z0-9._-]{1,20})?', v['adapter_firmware'])):
        raise ValueError('Invalid adapter firmware')
    if 'adapter_hardware' in v and (not isinstance(v['adapter_hardware'], str) or
            not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9 ._-]{0,31}', v['adapter_hardware'])):
        raise ValueError('Invalid adapter hardware')
    if 'recorded_by' in v and v['recorded_by'] not in IDENTITY_RECORDERS:
        raise ValueError('Invalid identity recorder')

IDENTITY_VALIDATORS = {'program_identity': validate_program_identity, 'shell_identity': validate_app_identity,
                       'preparer_identity': validate_app_identity, 'firmware_identity': validate_firmware_identity}

def require_identity(report):
    """Report format 2 must identify its preparer and how every session was launched.

    program_identity stays optional per session (the engine may never have started), but when
    present it must be schema 1. Format 1 keeps today's rules exactly.
    """
    p = report.get('preparer_identity')
    if not isinstance(p, dict) or p.get('identity_schema') != 1:
        raise ValueError('Report identity is required')
    sessions = report.get('recent_sessions', [])
    if not isinstance(sessions, list):
        raise ValueError('Report identity is required')
    for s in sessions:
        if not isinstance(s, dict) or not isinstance(s.get('shell_identity'), dict):
            raise ValueError('Session identity is required')
        program = s.get('program_identity')
        if program is not None and program.get('identity_schema') != 1:
            raise ValueError('Session identity is required')

def parse_uploader(raw):
    """Send-time uploader claim from a header. Returns (status, identity); never raises."""
    if raw is None:
        return 'absent', None
    try:
        if len(raw.encode('utf-8')) > UPLOADER_MAX_BYTES:
            raise ValueError('oversize')
        value = json.loads(raw)
        validate_app_identity(value)
        return 'present', value
    except (ValueError, TypeError, AttributeError, RecursionError):
        return 'invalid', None

def validate(value, depth=0):
    if depth > 8:
        raise ValueError('Report is too deeply nested')
    if isinstance(value, dict):
        validate_emulation(value)
        if "diagnostic_evidence" in value: validate_diagnostic(value["diagnostic_evidence"])
        if len(value) > 48 or any(k not in FIELDS for k in value):
            raise ValueError('Unsupported report fields')
        # Identity-only keys are accepted solely inside identity objects, never as free fields.
        if IDENTITY_ONLY & value.keys():
            raise ValueError('Unsupported report fields')
        for key in IDENTITY_CONTAINERS & value.keys():
            IDENTITY_VALIDATORS[key](value[key])
        if 'failure_stage' in value and (not isinstance(value['failure_stage'],str) or value['failure_stage'] not in IOS_FAILURE_STAGES):
            raise ValueError('Unsupported failure stage')
        if 'model' in value and (not isinstance(value['model'], str) or not re.fullmatch(r'[A-Za-z0-9,._-]{1,40}',value['model'])):
            raise ValueError('Unsupported device model')
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
                not re.fullmatch(r'(?:[a-f0-9]{64}|(?:chipsoft|native|offline|mdi|mongoose)-[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12})', value['session_id'])):
            raise ValueError('Unsupported session identifier')
        for key, limit in (('diagnostic_events', 32), ('failure_categories', 16)):
            if key in value and (not isinstance(value[key], list) or len(value[key]) > limit):
                raise ValueError('Too many diagnostic entries')
        if 'panic_location' in value:
            validate_panic_location(value['panic_location'])
        for key, child in value.items():
            # Identity containers and panic_location are validated strictly above.
            if key not in IDENTITY_CONTAINERS and key != 'panic_location':
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
    if not isinstance(report, dict):
        raise ValueError('Unsupported report format')
    if type(report.get('format')) is int and report['format'] == 2:
        require_identity(report)  # identity is enforced for the new format only
    elif report.get('format') != 1:  # legacy acceptance kept exactly as deployed
        raise ValueError('Unsupported report format')
    app = report.get('app')
    if not isinstance(report.get('description'), str):
        raise ValueError('Required report information is missing')
    if app in ANDROID_APPS:
        if not isinstance(report.get('android_api'), int):
            raise ValueError('Required report information is missing')
    elif app == IOS_APP:
        if report.get('platform') != 'ios' or report.get('arch') != 'arm64' or 'android_api' in report:
            raise ValueError('Required iOS report information is missing')
    elif app == DESKTOP_APP:
        if report.get('platform') not in ('macos', 'windows', 'linux') or report.get('arch') not in ('arm64', 'x86_64'):
            raise ValueError('Required report information is missing')
        if 'android_api' in report:
            raise ValueError('Unsupported report fields')
    else:
        raise ValueError('Unsupported application')
    return json.dumps(report, ensure_ascii=True, sort_keys=True, separators=(',', ':')).encode()

REPORT_PREFIX = 'support-reports/v1/'
INDEX_PREFIX = 'support-index/v1/'
SUBMISSION_PREFIX = 'support-submissions/v1/'
MANIFEST_KEY = 'support-manifests/v1/trusted.json'

def _utc_now():
    return datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S.%fZ')

def load_manifest(raw):
    """Trusted release manifest -> {sha256: entry}. Written only by release tooling, never by clients.

    {"schema":1,"entries":[
      {"sha256":"<64 hex>","kind":"executable"|"package","version":"<release version>","source_revision":"<40 hex>"},
      {"sha256":"<64 hex>","kind":"firmware","role":"card"|"eprom"|"opsys"|"candi","version":"<catalog id>",
       "source_revision":"unknown","aliases":["<other catalog id for identical bytes>", ...]}, ...]}

    Firmware entries bind image bytes to an image role and a catalog id; identical bytes legitimately
    published under several catalog ids must be listed explicitly as aliases. Any malformed or
    contradictory entry invalidates the whole manifest (fail closed: nothing verifies).
    """
    doc = json.loads(raw)
    if not isinstance(doc, dict) or doc.get('schema') != 1 or not isinstance(doc.get('entries'), list):
        raise ValueError('Invalid manifest')
    out = {}
    for e in doc['entries']:
        if (not isinstance(e, dict) or e.get('kind') not in ('executable', 'package', 'firmware')
                or not isinstance(e.get('sha256'), str) or not _SHA256_RE.fullmatch(e['sha256'])
                or not isinstance(e.get('source_revision'), str) or not _REVISION_RE.fullmatch(e['source_revision'])):
            raise ValueError('Invalid manifest entry')
        if e['kind'] == 'firmware':
            aliases = e.get('aliases', [])
            if (set(e) - {'sha256', 'kind', 'role', 'version', 'source_revision', 'aliases'}
                    or e.get('role') not in FIRMWARE_ROLES.values()
                    or not isinstance(e.get('version'), str) or not _CARD_ID_RE.fullmatch(e['version'])
                    or not isinstance(aliases, list) or len(aliases) > 16
                    or any(not isinstance(a, str) or not _CARD_ID_RE.fullmatch(a) for a in aliases)):
                raise ValueError('Invalid firmware manifest entry')
            entry = {'kind': 'firmware', 'role': e['role'], 'version': e['version'],
                     'source_revision': e['source_revision'], 'aliases': sorted(set(aliases))}
        else:
            if (set(e) - {'sha256', 'kind', 'version', 'source_revision'}
                    or not isinstance(e.get('version'), str) or not _VERSION_RE.fullmatch(e['version'])):
                raise ValueError('Invalid manifest entry')
            entry = {k: e[k] for k in ('kind', 'version', 'source_revision')}
        if e['sha256'] in out and out[e['sha256']] != entry:
            raise ValueError('Contradictory manifest entries')
        out[e['sha256']] = entry
    return out

def _manifest_match(manifest, digest, kind, version=None, revision=None):
    entry = manifest.get(digest)
    if entry is None:
        return 'unverified_claim'
    if entry['kind'] != kind or (version is not None and
                                 (entry['version'] != version or entry['source_revision'] != revision)):
        return 'manifest_mismatch'
    return 'verified'

def verify_identity(claim, manifest, role):
    """Well-formed is not authentic: only a manifest hash match with agreeing version/revision verifies.

    role 'program': the engine's own executable hash vs. the engine version.
    role 'app':     the installed package hash vs. the app version (never the engine crate version).
    """
    if claim is None:
        return 'absent'
    if not isinstance(claim, dict) or claim.get('identity_schema') != 1:
        return 'legacy_claim'
    if role == 'program':
        if claim.get('executable_present') is False:
            return 'executable_missing'
        digest, kind = claim.get('executable_sha256'), 'executable'
    else:
        digest, kind = claim.get('package_sha256'), 'package'
    if not digest:
        return 'unverified_claim'
    return _manifest_match(manifest, digest, kind, claim['version'], claim['source_revision'])

def verify_launch(claim, manifest):
    """Which engine binary an app found immediately before launching it (shell pre-launch record)."""
    if not isinstance(claim, dict) or claim.get('identity_schema') != 1:
        return 'absent'
    if claim.get('executable_present') is False:
        return 'executable_missing'
    digest = claim.get('executable_sha256')
    if not digest:
        return 'absent'
    result = _manifest_match(manifest, digest, 'executable')
    if result == 'verified' and 'engine_version' in claim and manifest[digest]['version'] != claim['engine_version']:
        return 'manifest_mismatch'
    return result

def verify_firmware(claim, manifest):
    """Per-image verification: each hash must be a trusted image in the SAME role, and the card image
    must match the claimed card id (or an explicit manifest alias). Adapter firmware is reported only.

    Returns {'verification', 'images': {role: result}, 'catalog_card_id'}.
    """
    if claim is None:
        return {'verification': 'absent', 'images': {}, 'catalog_card_id': None}
    images, catalog_card_id = {}, None
    for key, role in FIRMWARE_ROLES.items():
        digest = claim.get(key)
        if not digest:
            continue
        entry = manifest.get(digest)
        if entry is None:
            images[role] = 'unverified_claim'
        elif entry['kind'] != 'firmware' or entry['role'] != role:
            images[role] = 'manifest_mismatch'
        elif role == 'card' and 'card_id' in claim and claim['card_id'] not in {entry['version'], *entry['aliases']}:
            images[role] = 'manifest_mismatch'  # trusted bytes, but not the card the session claims it loaded
        else:
            images[role] = 'verified'
            if role == 'card':
                catalog_card_id = entry['version']
    results = set(images.values())
    overall = ('unverified_claim' if not images else 'manifest_mismatch' if 'manifest_mismatch' in results
               else 'unverified_claim' if 'unverified_claim' in results else 'verified')
    return {'verification': overall, 'images': images, 'catalog_card_id': catalog_card_id}

def launch_consistency(shell, program):
    """The core confirms the engine the shell recorded before launch."""
    pre = shell.get('executable_sha256') if isinstance(shell, dict) else None
    own = program.get('executable_sha256') if isinstance(program, dict) else None
    if not pre or not own:
        return 'unconfirmed'
    return 'consistent' if pre == own else 'mismatch'

def _claim(v):
    return v if isinstance(v, dict) else None

def build_index(report_id, report, manifest, manifest_status='loaded'):
    """Derived, recomputable identity summary. Never alters the stored report body or its receipt."""
    sessions = []
    rows = report.get('recent_sessions') if isinstance(report.get('recent_sessions'), list) else []
    for i, s in enumerate(rows):
        if not isinstance(s, dict):
            continue
        program, shell = _claim(s.get('program_identity')), _claim(s.get('shell_identity'))
        firmware = _claim(s.get('firmware_identity'))
        sessions.append({'index': i,
                         'session_id': s.get('session_id') if isinstance(s.get('session_id'), str) else None,
                         'program': {'claim': program, 'verification': verify_identity(program, manifest, 'program')},
                         'shell': {'claim': shell, 'verification': verify_identity(shell, manifest, 'app'),
                                   'launch_verification': verify_launch(shell, manifest)},
                         'firmware': {'claim': firmware, **verify_firmware(firmware, manifest)},
                         'launch_consistency': launch_consistency(shell, program)})
    top_program = _claim(report.get('program_identity'))
    preparer = _claim(report.get('preparer_identity'))
    schema1 = [c for c in [preparer, top_program] + [x['claim'] for s in sessions for x in (s['program'], s['shell'])]
               if c is not None and c.get('identity_schema') == 1]
    def v1(c):
        return isinstance(c, dict) and c.get('identity_schema') == 1
    # Complete: preparer known, every session's launch recorded, and the engine either confirmed
    # its own identity or the shell recorded that the engine executable was missing.
    complete = v1(preparer) and all(
        v1(s['shell']['claim']) and (v1(s['program']['claim']) or s['shell']['claim'].get('executable_present') is False)
        for s in sessions)
    classification = 'identity_v1' if complete else ('partial_identity' if schema1 else 'legacy_unverified')
    return {'schema': 1, 'report_id': report_id, 'indexed_utc': _utc_now(), 'classification': classification,
            'manifest_status': manifest_status, 'manifest_entries': len(manifest),
            'report_app': report.get('app') if isinstance(report.get('app'), str) else None,
            'report_version': report.get('version') if isinstance(report.get('version'), str) else None,
            'report_format': report.get('format') if type(report.get('format')) is int else None,
            'preparer': {'claim': preparer, 'verification': verify_identity(preparer, manifest, 'app'),
                         'engine_verification': verify_launch(preparer, manifest)},
            'program': {'claim': top_program, 'verification': verify_identity(top_program, manifest, 'program')}
                       if top_program is not None else None,
            'sessions': sessions}

def _put_json(client, bucket_name, key, doc):
    body = json.dumps(doc, ensure_ascii=True, sort_keys=True, separators=(',', ':')).encode()
    client.put_object(Bucket=bucket_name, Key=key, Body=body, ContentType='application/json',
                      Metadata={'sha256': hashlib.sha256(body).hexdigest()})

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

def make_router(storage, bucket, admin_token, manifest_source=None, manifest_ttl=600.0):
    router = APIRouter()
    limits = Limits()
    cache = {'at': None, 'manifest': {}, 'status': 'unavailable'}
    cache_lock = threading.Lock()

    def configured():
        client = storage()
        if client is None or not bucket() or not admin_token():
            raise HTTPException(503, 'Report storage is temporarily unavailable')
        return client

    def manifest():
        """Trusted manifest, cached. Any failure degrades to 'unverified', never blocks an upload."""
        with cache_lock:
            now = time.monotonic()
            if cache['at'] is not None and now - cache['at'] < manifest_ttl:
                return cache['manifest'], cache['status']
            try:
                if manifest_source is not None:
                    raw = manifest_source()
                else:
                    raw = storage().get_object(Bucket=bucket(), Key=MANIFEST_KEY)['Body'].read(4 * 1024 * 1024)
                cache.update(manifest=load_manifest(raw), status='loaded')
            except Exception:
                cache.update(manifest={}, status='unavailable')
            cache['at'] = now
            return cache['manifest'], cache['status']

    def record_sidecars(client, report_id, body, uploader_status, uploader):
        """Best-effort, written only after the report body is confirmed. Body and receipt never change."""
        try:
            _put_json(client, bucket(), SUBMISSION_PREFIX + report_id + '/' + _utc_now() + '-' + secrets.token_hex(4) + '.json',
                      {'schema': 1, 'report_id': report_id, 'received_utc': _utc_now(),
                       'uploader_status': uploader_status, 'uploader': uploader})
        except Exception:
            pass
        try:
            trusted, status = manifest()
            _put_json(client, bucket(), INDEX_PREFIX + report_id + '.json',
                      build_index(report_id, json.loads(body), trusted, status))
        except Exception:
            pass

    def save(body, uploader_status='absent', uploader=None):
        client = configured()
        digest = hashlib.sha256(body).hexdigest()
        report_id = 'OS-' + digest[:24]
        key = REPORT_PREFIX + report_id + '.json'
        # Content-addressed key: retrying the same frozen report keeps its number.
        try:
            client.put_object(Bucket=bucket(), Key=key, Body=body,
                              ContentType='application/json', Metadata={'sha256': digest})
            stored = client.head_object(Bucket=bucket(), Key=key)
            if stored.get('ContentLength') != len(body) or stored.get('Metadata', {}).get('sha256') != digest:
                raise RuntimeError('Storage verification failed')
        except Exception:
            raise HTTPException(503, 'Report was not confirmed; keep your local copy and retry') from None
        record_sidecars(client, report_id, body, uploader_status, uploader)
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
        # Uploader identity is send-time metadata, kept outside the frozen report bytes.
        uploader_status, uploader = parse_uploader(request.headers.get(UPLOADER_HEADER))
        return await run_in_threadpool(save, clean, uploader_status, uploader)

    def require_admin(request):
        expected = admin_token()
        supplied = request.headers.get('Authorization', '')
        if not expected or not secrets.compare_digest(supplied, 'Bearer ' + expected):
            raise HTTPException(401, 'Administrator access required')

    def admin(request, report_id):
        require_admin(request)
        if not re.fullmatch(r'OS-[a-f0-9]{24}', report_id):
            raise HTTPException(404, 'Report not found')

    @router.post('/api/admin/support/index/backfill')
    def backfill(request: Request, recompute: bool = False, limit: int = 100, start_after: str = ''):
        """Write identity-index sidecars for stored reports, one page per call. Report bodies are read-only here."""
        require_admin(request)
        if start_after and not re.fullmatch(r'support-reports/v1/OS-[a-f0-9]{24}\.json', start_after):
            raise HTTPException(400, 'Invalid continuation key')
        client = configured()
        trusted, status = manifest()
        listing = client.list_objects_v2(Bucket=bucket(), Prefix=REPORT_PREFIX, MaxKeys=max(1, min(limit, 200)),
                                         **({'StartAfter': start_after} if start_after else {}))
        result = {'scanned': 0, 'written': 0, 'skipped': 0, 'failed': 0, 'manifest_status': status, 'next_start_after': None}
        for item in sorted(listing.get('Contents', []), key=lambda o: o['Key']):
            key = item['Key']
            result['scanned'] += 1
            result['next_start_after'] = key
            m = re.fullmatch(r'support-reports/v1/(OS-[a-f0-9]{24})\.json', key)
            if not m:
                result['skipped'] += 1
                continue
            report_id = m.group(1)
            try:
                if not recompute:
                    try:
                        client.head_object(Bucket=bucket(), Key=INDEX_PREFIX + report_id + '.json')
                        result['skipped'] += 1
                        continue
                    except Exception:
                        pass  # no sidecar yet
                body = client.get_object(Bucket=bucket(), Key=key)['Body'].read(MAX_BYTES + 1)
                if len(body) > MAX_BYTES:
                    raise ValueError('Oversized stored object')
                index = build_index(report_id, json.loads(body), trusted, status)
                index['backfilled'] = True
                _put_json(client, bucket(), INDEX_PREFIX + report_id + '.json', index)
                result['written'] += 1
            except Exception:
                result['failed'] += 1
        if not listing.get('IsTruncated'):
            result['next_start_after'] = None
        return result

    @router.get('/api/admin/support/reports/{report_id}/identity')
    def identity(report_id: str, request: Request):
        """Identity index plus every recorded submission event for one receipt."""
        admin(request, report_id)
        client = configured()
        try:
            body = client.get_object(Bucket=bucket(), Key=REPORT_PREFIX + report_id + '.json')['Body'].read(MAX_BYTES + 1)
            if len(body) > MAX_BYTES:
                raise ValueError('Oversized stored object')
        except Exception:
            raise HTTPException(404, 'Report unavailable') from None
        try:
            index = json.loads(client.get_object(Bucket=bucket(), Key=INDEX_PREFIX + report_id + '.json')['Body'].read(MAX_BYTES + 1))
        except Exception:
            trusted, status = manifest()  # Legacy report without a sidecar: derive, do not write.
            index = build_index(report_id, json.loads(body), trusted, status)
            index['derived_on_read'] = True
        submissions = []
        try:
            listing = client.list_objects_v2(Bucket=bucket(), Prefix=SUBMISSION_PREFIX + report_id + '/', MaxKeys=200)
            for item in sorted(listing.get('Contents', []), key=lambda o: o['Key']):
                submissions.append(json.loads(client.get_object(Bucket=bucket(), Key=item['Key'])['Body'].read(UPLOADER_MAX_BYTES * 4)))
        except Exception:
            submissions = None
        return {'index': index, 'submissions': submissions}

    @router.get('/api/admin/support/reports/{report_id}')
    def retrieve(report_id: str, request: Request):
        admin(request, report_id)
        try:
            obj = configured().get_object(Bucket=bucket(), Key=REPORT_PREFIX + report_id + '.json')
            body = obj['Body'].read(MAX_BYTES + 1)
            if len(body) > MAX_BYTES:
                raise ValueError('Oversized stored object')
            return json.loads(body)
        except HTTPException:
            raise
        except Exception:
            raise HTTPException(404, 'Report unavailable') from None
    return router
