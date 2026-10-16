# SPDX-License-Identifier: MPL-2.0
"""Build-identity acceptance: creation identity is immutable, uploader is separate, receipts are stable."""
import copy
import io
import json
import unittest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from service.support_reports_api import make_router

ADMIN = {'Authorization': 'Bearer test-only-admin'}
CONSENT = {'X-OpenSAAB-Consent': 'support-report-v1'}
REV_A, REV_B = 'a' * 40, 'b' * 40
EXE_A, APK_A, APK_B = '1' * 64, '2' * 64, '3' * 64
EXE_Z, CARD_Y, EPROM, OPSYS = '4' * 64, '5' * 64, '6' * 64, '7' * 64


class Store:
    def __init__(self, fail_prefixes=()):
        self.objects = {}
        self.fail_prefixes = fail_prefixes
    def put_object(self, **kw):
        if any(kw['Key'].startswith(p) for p in self.fail_prefixes):
            raise RuntimeError('private server detail')
        self.objects[kw['Key']] = (kw['Body'], kw['Metadata'])
    def head_object(self, **kw):
        body, meta = self.objects[kw['Key']]
        return {'ContentLength': len(body), 'Metadata': meta}
    def get_object(self, **kw):
        return {'Body': io.BytesIO(self.objects[kw['Key']][0])}
    def list_objects_v2(self, **kw):
        keys = sorted(k for k in self.objects if k.startswith(kw['Prefix']) and k > kw.get('StartAfter', ''))
        limit = kw.get('MaxKeys', 1000)
        return {'Contents': [{'Key': k} for k in keys[:limit]], 'IsTruncated': len(keys) > limit}
    def keys(self, prefix):
        return sorted(k for k in self.objects if k.startswith(prefix))
    def json(self, key):
        return json.loads(self.objects[key][0])


def program(version='0.2.25-recovered-core.4', rev=REV_A, **extra):
    return dict({'identity_schema': 1, 'name': 'OpenSAAB core', 'version': version, 'architecture': 'arm64',
                 'source_revision': rev, 'channel': 'preview', 'source_dirty': False, 'build_epoch': 1760000000,
                 'executable_sha256': EXE_A, 'executable_present': True, 'recorded_by': 'core'}, **extra)


def app(version, rev, apk, **extra):
    return dict({'identity_schema': 1, 'package': 'com.opensaab.tech2', 'version': version, 'version_code': 70,
                 'source_revision': rev, 'channel': 'preview', 'package_sha256': apk, 'platform': 'android',
                 'architecture': 'arm64'}, **extra)


MANIFEST = json.dumps({'schema': 1, 'entries': [
    {'sha256': EXE_A, 'kind': 'executable', 'version': '0.2.25-recovered-core.4', 'source_revision': REV_A},
    {'sha256': APK_A, 'kind': 'package', 'version': '0.1.0-preview.70', 'source_revision': REV_A},
    {'sha256': APK_B, 'kind': 'package', 'version': '0.1.0-preview.90', 'source_revision': REV_B},
    {'sha256': EXE_Z, 'kind': 'executable', 'version': '0.2.26-recovered-core.1', 'source_revision': REV_B},
    {'sha256': CARD_Y, 'kind': 'firmware', 'role': 'card', 'version': 'tech2_card_saab_v148.000_en', 'source_revision': 'unknown'},
    {'sha256': EPROM, 'kind': 'firmware', 'role': 'eprom', 'version': 'eprom-v1', 'source_revision': 'unknown'},
    {'sha256': OPSYS, 'kind': 'firmware', 'role': 'opsys', 'version': 'opsys-v1', 'source_revision': 'unknown'},
]}).encode()
SWEDISH_148 = '9' * 64


def manifest_with(*entries):
    doc = json.loads(MANIFEST)
    doc['entries'].extend(entries)
    return json.dumps(doc).encode()


def firmware(**extra):
    return dict({'identity_schema': 1, 'card_sha256': CARD_Y, 'eprom_sha256': EPROM, 'opsys_sha256': OPSYS,
                 'card_id': 'tech2_card_saab_v148.000_en', 'card_bytes': 33554432, 'adapter_kind': 'nano',
                 'adapter_firmware': '1.9.4.2', 'adapter_hardware': 'VCX-NANO', 'recorded_by': 'core'}, **extra)


class Identity(unittest.TestCase):
    def setUp(self, manifest=MANIFEST, store=None):
        self.store = store or Store()
        source = (lambda: manifest) if manifest is not None else (lambda: (_ for _ in ()).throw(OSError('down')))
        api = FastAPI()
        api.include_router(make_router(lambda: self.store, lambda: 'private-test-bucket', lambda: 'test-only-admin',
                                       manifest_source=source))
        self.client = TestClient(api)

    def send(self, report, uploader=None, raw_header=None):
        headers = dict(CONSENT)
        if uploader is not None:
            headers['X-OpenSAAB-Uploader'] = json.dumps(uploader)
        if raw_header is not None:
            headers['X-OpenSAAB-Uploader'] = raw_header
        return self.client.post('/api/support/reports', json=report, headers=headers)

    def a_to_b_report(self):
        """Session created by app A (preview.70); report later prepared by app B (preview.90)."""
        return {'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 34, 'version': '0.1.0-preview.90',
                'description': 'Synthetic A->B upgrade test',
                'preparer_identity': app('0.1.0-preview.90', REV_B, APK_B, recorded_by='shell'),
                'recent_sessions': [{
                    'adapter': 'nano', 'session_id': 'native-00000000-0000-0000-0000-000000000001',
                    'program_identity': program(),
                    'shell_identity': app('0.1.0-preview.70', REV_A, APK_A, recorded_by='shell_prelaunch',
                                          executable_sha256=EXE_A, executable_present=True)}]}

    # --- Acceptance criterion -------------------------------------------------------------
    def test_session_keeps_creation_identity_and_uploader_is_separate(self):
        report = self.a_to_b_report()
        uploader_b = app('0.1.0-preview.90', REV_B, APK_B)
        first = self.send(report, uploader=uploader_b)
        self.assertEqual(201, first.status_code, first.text)
        rid = first.json()['report_id']

        stored = self.store.json('support-reports/v1/' + rid + '.json')
        self.assertEqual(report, stored)  # frozen body unchanged; session still says A
        self.assertEqual('0.1.0-preview.70', stored['recent_sessions'][0]['shell_identity']['version'])
        self.assertNotIn('uploader', json.dumps(stored))

        index = self.store.json('support-index/v1/' + rid + '.json')
        self.assertEqual('identity_v1', index['classification'])
        s = index['sessions'][0]
        self.assertEqual(('0.1.0-preview.70', 'verified'), (s['shell']['claim']['version'], s['shell']['verification']))
        self.assertEqual('verified', s['shell']['launch_verification'])  # trusted engine binary was launched
        self.assertEqual(('0.2.25-recovered-core.4', 'verified'), (s['program']['claim']['version'], s['program']['verification']))
        self.assertEqual(('0.1.0-preview.90', 'verified'), (index['preparer']['claim']['version'], index['preparer']['verification']))

        events = self.store.keys('support-submissions/v1/' + rid + '/')
        self.assertEqual(1, len(events))
        event = self.store.json(events[0])
        self.assertEqual(('present', '0.1.0-preview.90'), (event['uploader_status'], event['uploader']['version']))

        # Re-sending the same frozen report from a still-newer app: same receipt, body untouched,
        # a second submission event records the new uploader. Sessions are never relabelled.
        later = app('0.1.0-preview.91', REV_B, APK_B)
        again = self.send(report, uploader=later)
        self.assertEqual(rid, again.json()['report_id'])
        self.assertEqual(report, self.store.json('support-reports/v1/' + rid + '.json'))
        versions = sorted(self.store.json(k)['uploader']['version'] for k in self.store.keys('support-submissions/v1/' + rid + '/'))
        self.assertEqual(['0.1.0-preview.90', '0.1.0-preview.91'], versions)

        view = self.client.get('/api/admin/support/reports/' + rid + '/identity', headers=ADMIN)
        self.assertEqual(200, view.status_code)
        self.assertEqual(2, len(view.json()['submissions']))
        self.assertEqual(401, self.client.get('/api/admin/support/reports/' + rid + '/identity').status_code)

    def test_acceptance_appA_coreX_cardY_then_appB_coreZ(self):
        """Handoff acceptance case: the run keeps A/X/Y and exact hashes; B/Z are identified separately."""
        report = self.a_to_b_report()
        report['format'] = 2
        report['preparer_identity'].update(executable_sha256=EXE_Z, engine_version='0.2.26-recovered-core.1')
        report['recent_sessions'][0]['firmware_identity'] = firmware()
        uploader = app('0.1.0-preview.90', REV_B, APK_B, executable_sha256=EXE_Z, engine_version='0.2.26-recovered-core.1')
        first = self.send(report, uploader=uploader)
        self.assertEqual(201, first.status_code, first.text)
        rid = first.json()['report_id']
        session = self.store.json('support-reports/v1/' + rid + '.json')['recent_sessions'][0]
        self.assertEqual((APK_A, EXE_A, CARD_Y), (session['shell_identity']['package_sha256'],
                         session['program_identity']['executable_sha256'], session['firmware_identity']['card_sha256']))
        index = self.store.json('support-index/v1/' + rid + '.json')
        s = index['sessions'][0]
        self.assertEqual(('identity_v1', 2), (index['classification'], index['report_format']))
        self.assertEqual(['verified'] * 4, [s['shell']['verification'], s['program']['verification'],
                                            s['firmware']['verification'], s['shell']['launch_verification']])
        self.assertEqual('consistent', s['launch_consistency'])  # core confirmed what the shell launched
        self.assertEqual(('0.1.0-preview.90', 'verified', 'verified'),
                         (index['preparer']['claim']['version'], index['preparer']['verification'],
                          index['preparer']['engine_verification']))
        event = self.store.json(self.store.keys('support-submissions/v1/' + rid + '/')[0])
        self.assertEqual('0.2.26-recovered-core.1', event['uploader']['engine_version'])
        self.assertEqual(rid, self.send(report, uploader=uploader).json()['report_id'])  # retry keeps receipt

    def test_format_2_enforces_identity_and_format_1_is_unchanged(self):
        ok = self.a_to_b_report()
        ok['format'] = 2
        missing_engine = copy.deepcopy(ok)  # engine never started: allowed, recorded by the shell
        missing_engine['recent_sessions'][0]['shell_identity'].update(executable_present=False)
        del missing_engine['recent_sessions'][0]['shell_identity']['executable_sha256']
        del missing_engine['recent_sessions'][0]['program_identity']
        for report in (ok, missing_engine):
            self.setUp()
            res = self.send(report)
            self.assertEqual(201, res.status_code, res.text)
            self.assertEqual('identity_v1', self.store.json('support-index/v1/' + res.json()['report_id'] + '.json')['classification'])
        no_preparer = copy.deepcopy(ok); del no_preparer['preparer_identity']
        no_shell = copy.deepcopy(ok); del no_shell['recent_sessions'][0]['shell_identity']
        legacy_program = copy.deepcopy(ok)
        legacy_program['recent_sessions'][0]['program_identity'] = {'name': 'n', 'version': '1.0', 'architecture': 'arm64'}
        for bad in (no_preparer, no_shell, legacy_program, dict(ok, format=3), dict(ok, format='2')):
            self.setUp()
            self.assertEqual(400, self.send(bad).status_code)
        self.setUp()  # format 1 without identity still accepted exactly as deployed
        self.assertEqual(201, self.send({'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 34,
                                         'description': 'x', 'recent_sessions': [{'adapter': 'nano'}]}).status_code)

    def test_current_android_panic_and_offline_reports_survive_identity(self):
        """Preview .92 reports (panic_location, offline_native/mxp_research) in format 1 and 2."""
        where = {'file': 'emulator/src/chipsoft.rs', 'line': 412, 'column': 17, 'thread': 'main'}
        def current(fmt, profile):
            report = self.a_to_b_report()
            report['format'] = fmt
            session = report['recent_sessions'][0]
            session['logs'] = [{'source': 'console.log', 'file_bytes': 3145728, 'tail_only': True,
                                'event_counts': {'PANIC': 1}, 'panic_location': where}]
            session['emulation_context'] = {'schema': 1, 'profile': profile, 'started_utc': '2026-10-16T20:00:00Z'}
            return report
        for fmt in (1, 2):
            for profile in ('offline_research', 'offline_native', 'mxp_research'):
                with self.subTest(format=fmt, profile=profile):
                    self.setUp()
                    res = self.send(current(fmt, profile))
                    self.assertEqual(201, res.status_code, res.text)
                    stored = self.store.json('support-reports/v1/' + res.json()['report_id'] + '.json')
                    self.assertEqual(where, stored['recent_sessions'][0]['logs'][0]['panic_location'])
        for bad in (dict(where, message='VIN YS3F'), dict(where, file='../../etc/passwd')):
            self.setUp()
            report = current(2, 'offline_native')
            report['recent_sessions'][0]['logs'][0]['panic_location'] = bad
            self.assertEqual(400, self.send(report).status_code)
        self.setUp()  # Identity containers may not smuggle panic keys either.
        report = current(2, 'offline_native')
        report['preparer_identity']['file'] = 'x'
        self.assertEqual(400, self.send(report).status_code)

    def test_firmware_identity_strictness_and_launch_mismatch(self):
        report = self.a_to_b_report()
        report['recent_sessions'][0]['program_identity']['executable_sha256'] = EXE_Z  # core reports a different engine
        report['recent_sessions'][0]['firmware_identity'] = firmware(card_sha256='8' * 64)  # unknown card image
        rid = self.send(report).json()['report_id']
        s = self.store.json('support-index/v1/' + rid + '.json')['sessions'][0]
        self.assertEqual('mismatch', s['launch_consistency'])
        self.assertEqual('unverified_claim', s['firmware']['verification'])
        for bad in [firmware(card_sha256='abc'), firmware(card_id='../private path'), firmware(adapter_kind='other'),
                    firmware(adapter_firmware='raw text'), firmware(card_bytes=0), firmware(vin='private'),
                    firmware(identity_schema=2), {'card_sha256': CARD_Y}]:
            with self.subTest(bad=bad):
                self.setUp()
                r = self.a_to_b_report()
                r['recent_sessions'][0]['firmware_identity'] = bad
                self.assertEqual(400, self.send(r).status_code)

    def test_trusted_card_bytes_with_the_wrong_card_id_do_not_verify(self):
        """Review finding 1: a trusted Swedish-148 image claimed as NAO 9.250 must not be 'verified'."""
        self.setUp(manifest=manifest_with({'sha256': SWEDISH_148, 'kind': 'firmware', 'role': 'card',
                                           'version': 'saab-148-swedish', 'source_revision': 'unknown'}))
        def index_for(fw):
            r = self.a_to_b_report()
            r['recent_sessions'][0]['firmware_identity'] = fw
            res = self.send(r)
            self.assertEqual(201, res.status_code, res.text)
            return self.store.json('support-index/v1/' + res.json()['report_id'] + '.json')['sessions'][0]['firmware']
        wrong = index_for(firmware(card_sha256=SWEDISH_148, card_id='saab-nao-9.250-english'))
        self.assertEqual(('manifest_mismatch', 'manifest_mismatch', None),
                         (wrong['verification'], wrong['images']['card'], wrong['catalog_card_id']))
        self.assertEqual('verified', wrong['images']['eprom'])  # per-image results stay distinct
        right = index_for(firmware(card_sha256=SWEDISH_148, card_id='saab-148-swedish'))
        self.assertEqual(('verified', 'saab-148-swedish'), (right['verification'], right['catalog_card_id']))
        hash_only = dict(firmware(card_sha256=SWEDISH_148)); del hash_only['card_id']
        self.assertEqual(('verified', 'saab-148-swedish'), (index_for(hash_only)['verification'], index_for(hash_only)['catalog_card_id']))
        label_only = {'identity_schema': 1, 'card_id': 'saab-148-swedish'}  # a label alone proves nothing
        self.assertEqual('unverified_claim', index_for(label_only)['verification'])

    def test_explicit_aliases_and_image_roles(self):
        self.setUp(manifest=manifest_with({'sha256': SWEDISH_148, 'kind': 'firmware', 'role': 'card', 'version': 'saab-148-swedish',
                                           'source_revision': 'unknown', 'aliases': ['tech2_card_saab_v148.000_se']}))
        r = self.a_to_b_report()
        r['recent_sessions'][0]['firmware_identity'] = firmware(card_sha256=SWEDISH_148, card_id='tech2_card_saab_v148.000_se')
        rid = self.send(r).json()['report_id']
        self.assertEqual('verified', self.store.json('support-index/v1/' + rid + '.json')['sessions'][0]['firmware']['verification'])
        self.setUp()
        swapped = self.a_to_b_report()  # a trusted eprom image presented in the card slot
        swapped['recent_sessions'][0]['firmware_identity'] = firmware(card_sha256=EPROM)
        rid = self.send(swapped).json()['report_id']
        fw = self.store.json('support-index/v1/' + rid + '.json')['sessions'][0]['firmware']
        self.assertEqual(('manifest_mismatch', 'manifest_mismatch'), (fw['verification'], fw['images']['card']))

    def test_malformed_or_contradictory_manifest_fails_closed(self):
        bad_manifests = [
            manifest_with({'sha256': SWEDISH_148, 'kind': 'firmware', 'version': 'no-role', 'source_revision': 'unknown'}),
            manifest_with({'sha256': CARD_Y, 'kind': 'firmware', 'role': 'card', 'version': 'other-card', 'source_revision': 'unknown'}),
            manifest_with({'sha256': SWEDISH_148, 'kind': 'package', 'role': 'card', 'version': '1.0.0', 'source_revision': REV_A}),
            manifest_with({'sha256': SWEDISH_148, 'kind': 'package', 'version': 'SYNTHETIC_PRIVATE_TOKEN', 'source_revision': REV_A}),
        ]
        for raw in bad_manifests:
            with self.subTest():
                self.setUp(manifest=raw)
                rid = self.send(self.a_to_b_report()).json()['report_id']
                index = self.store.json('support-index/v1/' + rid + '.json')
                self.assertEqual(('unavailable', 'unverified_claim'), (index['manifest_status'], index['preparer']['verification']))

    def test_identity_versions_follow_release_grammar(self):
        for version in ('SYNTHETIC_PRIVATE_TOKEN', 'private name', '1', 'x1.0.0', '1.0.0-' + 'a' * 70):
            with self.subTest(version=version):
                self.setUp()
                r = self.a_to_b_report()
                r['preparer_identity']['version'] = version
                self.assertEqual(400, self.send(r).status_code)
        for version in ('0.1-dev', 'development', 'unknown', '0.2.26-intel-preview.7', '0.2.18'):
            with self.subTest(version=version):
                self.setUp()
                r = self.a_to_b_report()
                r['preparer_identity']['version'] = version
                self.assertEqual(201, self.send(r).status_code)

    # --- Receipt semantics ----------------------------------------------------------------
    def test_legacy_receipts_are_byte_identical_to_live_validator(self):
        # Receipts pinned from the live validator (commit 516dd7e, matching the deployed image).
        expected = ['OS-913fe5c07dfe21f7604a73a6', 'OS-decdecd564bb561b0655aa46', 'OS-42675771df03be02a4af382e']
        legacy = [
            {'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 34, 'version': '0.1.0-preview.70',
             'description': 'legacy', 'recent_sessions': [{'adapter': 'nano', 'emulator_outcome': {'exit_code': 3, 'instructions': 80000000}}]},
            {'format': 1, 'app': 'com.opensaab.desktop', 'platform': 'macos', 'arch': 'x86_64', 'version': '0.2.23',
             'build': 'abc', 'description': 'legacy desktop', 'recent_sessions': []},
            {'format': 1, 'app': 'com.opensaab.t2.ios.preview', 'platform': 'ios', 'arch': 'arm64', 'description': 'x',
             'model': 'iPhone17,1', 'failure_stage': 'ui_unresponsive'},
        ]
        for r, receipt in zip(legacy, expected):
            res = self.send(r)
            self.assertEqual(201, res.status_code, res.text)
            self.assertEqual(receipt, res.json()['report_id'])
            self.assertEqual(r, self.store.json('support-reports/v1/' + receipt + '.json'))

    def test_legacy_report_is_accepted_indexed_and_never_moved(self):
        report = {'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 34, 'version': '0.1.0-preview.70',
                  'description': 'legacy', 'recent_sessions': [{'adapter': 'nano'}]}
        res = self.send(report)
        self.assertEqual(201, res.status_code)
        rid = res.json()['report_id']
        self.assertEqual(report, self.store.json('support-reports/v1/' + rid + '.json'))  # stays in monitored prefix
        index = self.store.json('support-index/v1/' + rid + '.json')
        self.assertEqual('legacy_unverified', index['classification'])
        self.assertEqual('absent', index['sessions'][0]['program']['verification'])
        event = self.store.json(self.store.keys('support-submissions/v1/' + rid + '/')[0])
        self.assertEqual(('absent', None), (event['uploader_status'], event['uploader']))

    def test_existing_canonical_program_identity_is_a_legacy_claim(self):
        report = {'format': 1, 'app': 'com.opensaab.desktop', 'platform': 'macos', 'arch': 'arm64', 'description': 'x',
                  'recent_sessions': [{'adapter': 'nano', 'program_identity': {
                      'name': 'OpenSAAB 0.2.25-recovered-core.4 Apple silicon', 'version': '0.2.25-recovered-core.4',
                      'architecture': 'arm64'}}]}
        res = self.send(report)
        self.assertEqual(201, res.status_code, res.text)
        index = self.store.json('support-index/v1/' + res.json()['report_id'] + '.json')
        self.assertEqual('legacy_claim', index['sessions'][0]['program']['verification'])
        self.assertEqual('legacy_unverified', index['classification'])

    # --- Claim vs verification ------------------------------------------------------------
    def test_wellformed_claims_are_not_verified_without_manifest_match(self):
        report = self.a_to_b_report()
        report['recent_sessions'][0]['shell_identity']['package_sha256'] = '9' * 64  # not in any manifest
        report['recent_sessions'][0]['program_identity']['version'] = '0.2.99-forged'  # hash known, version disagrees
        report['preparer_identity']['package_sha256'] = EXE_A  # an executable hash presented as a package
        rid = self.send(report).json()['report_id']
        index = self.store.json('support-index/v1/' + rid + '.json')
        s = index['sessions'][0]
        self.assertEqual('unverified_claim', s['shell']['verification'])
        self.assertEqual('manifest_mismatch', s['program']['verification'])
        self.assertEqual('manifest_mismatch', index['preparer']['verification'])

    def test_manifest_outage_degrades_to_unverified_and_never_blocks(self):
        self.setUp(manifest=None)
        res = self.send(self.a_to_b_report())
        self.assertEqual(201, res.status_code)
        index = self.store.json('support-index/v1/' + res.json()['report_id'] + '.json')
        self.assertEqual('unavailable', index['manifest_status'])
        self.assertEqual('unverified_claim', index['sessions'][0]['program']['verification'])

    def test_missing_executable_is_recorded_before_launch(self):
        report = self.a_to_b_report()
        shell = report['recent_sessions'][0]['shell_identity']
        del shell['executable_sha256']
        shell['executable_present'] = False
        del report['recent_sessions'][0]['program_identity']  # core never ran
        rid = self.send(report).json()['report_id']
        s = self.store.json('support-index/v1/' + rid + '.json')['sessions'][0]
        self.assertEqual('verified', s['shell']['verification'])           # the app itself is genuine
        self.assertEqual('executable_missing', s['shell']['launch_verification'])  # but its engine was absent
        self.assertEqual('absent', s['program']['verification'])            # core never ran
        bad = copy.deepcopy(report)
        bad['recent_sessions'][0]['shell_identity']['executable_sha256'] = EXE_A  # missing + fingerprint is contradictory
        self.assertEqual(400, self.send(bad).status_code)

    # --- Strict shapes and privacy --------------------------------------------------------
    def test_identity_fields_are_strict_and_not_free_text(self):
        def with_session(**fields):
            r = self.a_to_b_report()
            r['recent_sessions'][0].update(fields)
            return r
        bad = [
            dict(self.a_to_b_report(), source_revision=REV_A),            # identity key outside identity object
            with_session(name='free text'),
            with_session(program_identity=program(vin='private')),
            with_session(program_identity=program(source_revision='deadbeef')),
            with_session(program_identity=program(channel='nightly')),
            with_session(program_identity=program(executable_sha256='XYZ')),
            with_session(program_identity=program(identity_schema=2)),
            with_session(program_identity=program(identity_schema=True)),
            with_session(program_identity=program(version='x' * 80)),
            with_session(program_identity={'name': 'n', 'version': '1', 'architecture': 'arm64', 'extra': 1}),
            with_session(shell_identity=app('0.1', REV_A, APK_A, package='com.other.app')),
            with_session(shell_identity=app('0.1', REV_A, APK_A, build_epoch=-1)),
        ]
        for i, report in enumerate(bad):
            with self.subTest(i=i):
                self.setUp()
                self.assertEqual(400, self.send(report).status_code)
                self.assertFalse(self.store.objects)

    def test_bad_uploader_header_is_recorded_as_invalid_without_content(self):
        for raw in ['not json', json.dumps(app('0.1', REV_B, APK_B, vin='private')), 'x' * 2000,
                    json.dumps({'identity_schema': 1})]:
            with self.subTest(raw=raw[:20]):
                self.setUp()
                res = self.send(self.a_to_b_report(), raw_header=raw)
                self.assertEqual(201, res.status_code)
                event = self.store.json(self.store.keys('support-submissions/v1/')[0])
                self.assertEqual(('invalid', None), (event['uploader_status'], event['uploader']))
                self.assertNotIn('private', json.dumps(event))

    # --- Backfill of reports stored before this change -------------------------------------
    def test_backfill_indexes_legacy_reports_without_touching_bodies(self):
        legacy = [{'format': 1, 'app': 'com.opensaab.tech2', 'android_api': 34, 'version': '0.1.0-preview.%d' % n,
                   'description': 'old %d' % n, 'recent_sessions': [{'adapter': 'nano'}]} for n in range(5)]
        receipts = [self.send(r).json()['report_id'] for r in legacy]
        for rid in receipts:  # simulate the pre-change store: bodies only, no sidecars
            del self.store.objects['support-index/v1/' + rid + '.json']
        bodies_before = {k: v for k, v in self.store.objects.items() if k.startswith('support-reports/')}
        route = '/api/admin/support/index/backfill'
        self.assertEqual(401, self.client.post(route).status_code)
        self.assertEqual(400, self.client.post(route, params={'start_after': '../x'}, headers=ADMIN).status_code)
        pages, after = [], ''
        while True:
            page = self.client.post(route, params={'limit': 2, 'start_after': after}, headers=ADMIN).json()
            pages.append(page)
            if not page['next_start_after']:
                break
            after = page['next_start_after']
        self.assertEqual(5, sum(p['written'] for p in pages))
        self.assertEqual(3, len(pages))
        for rid in receipts:
            index = self.store.json('support-index/v1/' + rid + '.json')
            self.assertEqual(('legacy_unverified', True), (index['classification'], index['backfilled']))
        self.assertEqual(bodies_before, {k: v for k, v in self.store.objects.items() if k.startswith('support-reports/')})
        again = self.client.post(route, params={'limit': 200}, headers=ADMIN).json()
        self.assertEqual((0, 5), (again['written'], again['skipped']))  # idempotent without recompute
        forced = self.client.post(route, params={'limit': 200, 'recompute': True}, headers=ADMIN).json()
        self.assertEqual(5, forced['written'])

    # --- Failure isolation ----------------------------------------------------------------
    def test_sidecar_failure_never_loses_the_report(self):
        self.setUp(store=Store(fail_prefixes=('support-index/', 'support-submissions/')))
        res = self.send(self.a_to_b_report())
        self.assertEqual(201, res.status_code)
        self.assertEqual(1, len(self.store.keys('support-reports/v1/')))
        view = self.client.get('/api/admin/support/reports/' + res.json()['report_id'] + '/identity', headers=ADMIN)
        self.assertTrue(view.json()['index']['derived_on_read'])

    def test_report_failure_writes_no_sidecars(self):
        self.setUp(store=Store(fail_prefixes=('support-reports/',)))
        self.assertEqual(503, self.send(self.a_to_b_report()).status_code)
        self.assertFalse(self.store.objects)


if __name__ == '__main__':
    unittest.main()
