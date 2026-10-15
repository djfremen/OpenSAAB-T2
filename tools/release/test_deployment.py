"""Synthetic gate and publisher regressions; never exercise GitHub or hardware."""
import copy
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import deployment
import publish


class DeploymentFixture:
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.version = '0.3.0-synthetic.1'
        self.sha = 'a' * 40
        self.capabilities = ['synthetic offline fixture; no hardware result']
        package_name = f'OpenSAAB-{self.version}-macos-x86_64.fixture'
        manifest_name = f'OpenSAAB-{self.version}-macos-x86_64-manifest.json'
        def write(name, content, role=None):
            path = self.root / name
            path.write_bytes(content)
            ref = {'path': name, 'sha256': deployment.digest(path)}
            if role:
                ref.update(size=path.stat().st_size, role=role)
            return ref
        self.write = write
        package = write(package_name, b'SYNTHETIC PACKAGE ONLY', 'package')
        source = write('matching-source.fixture', b'SYNTHETIC SOURCE ONLY', 'source')
        manifest = write(manifest_name, json.dumps({
            'product': 'OpenSAAB', 'version': self.version, 'source_revision': self.sha,
            'platform': 'macos', 'architecture': 'x86_64',
            'artifact_sha256': package['sha256'], 'capabilities': self.capabilities,
            'security_access': {'policy': deployment.SECURITY_POLICY, 'mode': 'password_protected'},
        }).encode(), 'manifest')
        proof = write('proof.txt', b'SYNTHETIC ASSERTIONS ONLY; NOT PHYSICAL EVIDENCE')
        self.security_proof = {
            'policy': deployment.SECURITY_POLICY, 'mode': 'password_protected',
            'version': self.version, 'source_revision': self.sha,
            'platform': 'macos', 'architecture': 'x86_64',
            'artifact_sha256': package['sha256'], 'status': 'pass',
            'reviewed_by': 'synthetic fixture only', 'observations': [proof],
            'password_sha256': deployment.SECURITY_PASSWORD_SHA256,
            'cases': {name: {'result': result, 'note': 'Synthetic fixture only; no installed test'}
                      for name, result in deployment.SECURITY_CASES.items()},
        }
        security_evidence = write('security-proof.json', json.dumps(self.security_proof).encode())
        contract = write('interface-contract.json', (deployment.ROOT / 'emulator/ui/interface-contract.json').read_bytes())
        self.data = {
            'schema_version': 1, 'kind': 'application', 'product': 'OpenSAAB',
            'version': self.version, 'channel': 'preview', 'reviewed_by': 'synthetic test',
            'source': {'revision': self.sha, 'dirty': False, 'reviewed_by': 'synthetic test'},
            'interface_contract': contract,
            'notes': write('NOTES.md', (self.version + '\nSynthetic fixture only.').encode()),
            'release': {'repository': 'example/synthetic', 'tag': 'v' + self.version, 'title': 'OpenSAAB ' + self.version},
            'artifacts': [package, source, manifest],
            'targets': [{
                'platform': 'macos', 'architecture': 'x86_64', 'artifact': package_name,
                'manifest': manifest_name, 'capabilities': self.capabilities,
                'security_access': {'policy': deployment.SECURITY_POLICY,
                                    'mode': 'password_protected', 'evidence': security_evidence},
                'identity': {
                    'artifact_sha256': package['sha256'], 'version_source': 'packaged_metadata',
                    'installed_name': f'OpenSAAB-{self.version}-macos-x86_64.app',
                    'program_name': f'OpenSAAB {self.version} Intel',
                    'window_title': f'OpenSAAB {self.version} Intel',
                    'installer_name': package_name, 'manifest_name': manifest_name, 'evidence': proof,
                },
            }],
            'checks': {name: {'status': 'pass', 'note': 'Synthetic fixture only', 'evidence': [proof]} for name in deployment.CHECKS},
            'surfaces': [{'name': name, 'action': 'unchanged', 'note': 'Synthetic isolated fixture; no public update', 'evidence': proof} for name in sorted(deployment.SURFACES)],
            'rollback': {'target': 'previous synthetic fixture', 'procedure': 'select the retained previous fixture'},
        }
        self.data['checks']['deployment'] = {'status': 'pending', 'note': 'Live verification follows publication', 'evidence': []}
        self.ledger = self.root / 'ledger.json'

    def errors(self, data=None, phase='prepare'):
        return deployment.validate(data or self.data, self.root, phase, deployment.load_ledger(self.ledger))

    def bind_security_proof(self, proof):
        self.data['targets'][0]['security_access']['evidence'] = self.write(
            'security-proof.json', json.dumps(proof).encode())


class DeploymentTests(DeploymentFixture, unittest.TestCase):
    def test_owner_dated_android_arm64_filename_is_scoped_to_installer_and_manifest(self):
        version = '0.1.1-preview.3'
        package = 'OpenSAAB_64_14OCT26_v' + version + '.apk'
        manifest = 'OpenSAAB_64_14OCT26_v' + version + '-manifest.json'
        matches = deployment.identity_architecture_matches
        self.assertTrue(matches(package, 'android', 'arm64', 'installer_name', version))
        self.assertTrue(matches(manifest, 'android', 'arm64', 'manifest_name', version))
        self.assertFalse(matches(package, 'android', 'arm64', 'program_name', version))
        self.assertFalse(matches(package, 'macos', 'arm64', 'installer_name', version))
        self.assertFalse(matches(package, 'android', 'x86_64', 'installer_name', version))
        self.assertFalse(matches(package.replace('14OCT26', '99OCT26'), 'android', 'arm64', 'installer_name', version))
        self.assertFalse(matches(package.replace('_64_', '_32_'), 'android', 'arm64', 'installer_name', version))
        self.assertFalse(matches('OpenSAAB_64_' + version + '.apk', 'android', 'arm64', 'installer_name', version))

    def test_canonical_password_policy_agrees_with_release_gate(self):
        import hashlib
        contract = json.loads((deployment.ROOT / 'emulator/ui/interface-contract.json').read_text())
        policy = contract['security_workflow']['password_policy']
        self.assertEqual(policy['id'], deployment.SECURITY_POLICY)
        self.assertIs(policy['case_sensitive'], True)
        self.assertEqual(hashlib.sha256(policy['password'].encode()).hexdigest(), deployment.SECURITY_PASSWORD_SHA256)

    def test_security_password_evidence_is_mandatory_for_previews(self):
        del self.data['targets'][0]['security_access']
        self.assertTrue(any('security access' in e for e in self.errors()))

    def test_security_password_cases_cannot_be_skipped_or_marked_pass_without_scope(self):
        for name in deployment.SECURITY_CASES:
            for change in ('missing', 'wrong_result', 'empty_note'):
                proof = copy.deepcopy(self.security_proof)
                if change == 'missing':
                    del proof['cases'][name]
                elif change == 'wrong_result':
                    proof['cases'][name]['result'] = 'accepted' if name != 'correct_password' else 'blocked'
                else:
                    proof['cases'][name]['note'] = ''
                self.bind_security_proof(proof)
                self.assertTrue(any(name in e for e in self.errors()), (name, change))

    def test_security_password_must_be_exact_and_case_sensitive(self):
        proof = copy.deepcopy(self.security_proof)
        proof['password_sha256'] = 'b' * 64
        self.bind_security_proof(proof)
        self.assertTrue(any('exactly OpenSAAB' in e for e in self.errors()))

    def test_security_evidence_cannot_be_pending_or_from_another_artifact(self):
        for field, value in [('status', 'pending'), ('artifact_sha256', 'b' * 64),
                             ('version', '0.1.0-other.1'), ('architecture', 'arm64'),
                             ('platform', 'ios'), ('source_revision', 'b' * 40),
                             ('policy', 'another-policy'), ('reviewed_by', '')]:
            proof = copy.deepcopy(self.security_proof)
            proof[field] = value
            self.bind_security_proof(proof)
            self.assertTrue(any('security access' in e for e in self.errors()), field)

    def test_security_evidence_requires_retained_observations(self):
        proof = copy.deepcopy(self.security_proof)
        proof['observations'] = []
        self.bind_security_proof(proof)
        self.assertTrue(any('observations' in e for e in self.errors()))
        proof['observations'] = [{'path': 'missing.txt', 'sha256': 'b' * 64}]
        self.bind_security_proof(proof)
        self.assertTrue(any('security access observation' in e for e in self.errors()))

    def test_malformed_security_evidence_fails_closed(self):
        for value in ('not json', '[]', 'null', '{"cases":[]}'):
            self.data['targets'][0]['security_access']['evidence'] = self.write('invalid-security.json', value.encode())
            self.assertTrue(any('security access' in e for e in self.errors()))
        self.data['targets'][0]['security_access']['mode'] = []
        self.assertTrue(any('invalid protection mode' in e for e in self.errors()))

    def test_unavailable_security_requires_explicit_evidence_and_matching_manifest(self):
        target = self.data['targets'][0]
        target['security_access']['mode'] = 'unavailable'
        proof = copy.deepcopy(self.security_proof)
        proof.update(mode='unavailable', cases={'all_security_entry_points': {
            'result': 'unavailable', 'note': 'Synthetic unavailable feature fixture only'}})
        self.bind_security_proof(proof)
        self.assertTrue(any('public manifest' in e for e in self.errors()))
        ref = self.data['artifacts'][2]
        metadata = json.loads((self.root / ref['path']).read_text())
        metadata['security_access'] = deployment.security_summary(target)
        self.data['artifacts'][2] = self.write(ref['path'], json.dumps(metadata).encode(), 'manifest')
        self.assertEqual(self.errors(), [])
        proof['cases']['all_security_entry_points']['result'] = 'accepted'
        self.bind_security_proof(proof)
        self.assertTrue(any('all_security_entry_points' in e for e in self.errors()))

    def test_security_mode_changes_cannot_reuse_a_reserved_version(self):
        deployment.reserve(self.data, self.ledger)
        self.data['targets'][0]['security_access']['mode'] = 'unavailable'
        self.assertTrue(any('changed security-access policy' in e for e in self.errors()))
        with self.assertRaises(ValueError):
            deployment.reserve(self.data, self.ledger)

    def test_synthetic_record_prepares_but_cannot_claim_completion(self):
        self.assertEqual(self.errors(), [])
        self.assertTrue(self.errors(phase='complete'))
        self.data['checks']['deployment'] = copy.deepcopy(self.data['checks']['build'])
        self.assertEqual(self.errors(phase='complete'), [])

    def test_incomplete_template_and_missing_commandment_are_blocked(self):
        self.assertTrue(self.errors(deployment.template()))
        for name in deployment.CHECKS:
            data = copy.deepcopy(self.data)
            del data['checks'][name]
            self.assertTrue(self.errors(data), name)

    def test_file_changes_wrong_sizes_and_escaping_paths_are_blocked(self):
        for change in ('hash', 'size', 'escape', 'symlink'):
            data = copy.deepcopy(self.data)
            if change == 'hash':
                data['artifacts'][0]['sha256'] = 'b' * 64
            elif change == 'size':
                data['artifacts'][0]['size'] += 1
            elif change == 'escape':
                data['artifacts'][0]['path'] = '../outside'
            else:
                link = self.root / 'outside-link'
                link.symlink_to('/etc/hosts')
                data['artifacts'][0]['path'] = link.name
            self.assertTrue(self.errors(data), change)

    def test_changed_evidence_and_contract_are_blocked(self):
        (self.root / 'proof.txt').write_bytes(b'changed evidence')
        self.assertTrue(self.errors())
        self.setUp()
        self.data['interface_contract'] = self.write('other-contract.json', b'{}')
        self.assertTrue(self.errors())

    def test_identity_checks_every_visible_name_architecture_and_artifact(self):
        for field in ('installed_name', 'program_name', 'window_title', 'installer_name', 'manifest_name'):
            data = copy.deepcopy(self.data)
            data['targets'][0]['identity'][field] = 'OpenSAAB.app'
            self.assertTrue(self.errors(data), field)
        for field, value in [('artifact_sha256', 'b' * 64), ('version_source', 'filename')]:
            data = copy.deepcopy(self.data)
            data['targets'][0]['identity'][field] = value
            self.assertTrue(self.errors(data))

    def test_manifest_and_each_package_must_match_target(self):
        self.data['targets'][0]['capabilities'] = ['changed capability']
        self.assertTrue(self.errors())
        self.setUp()
        self.data['artifacts'].append(self.write('uncovered.fixture', b'UNVERIFIED TARGET', 'package'))
        self.assertTrue(self.errors())

    def test_preview_pending_physical_scope_must_be_published_verbatim(self):
        check = self.data['checks']['artifact_tests']
        check.update(status='pending', note='Physical adapter checks remain pending for this exact package.')
        self.assertTrue(self.errors())
        self.data['notes'] = self.write('NOTES.md', (self.version + '\n' + check['note']).encode())
        self.assertEqual(self.errors(), [])
        self.data['channel'] = 'qualified'
        self.assertTrue(self.errors())

    def test_no_checkbox_skip_or_missing_publication_surface(self):
        self.data['checks']['source']['status'] = 'not_applicable'
        self.assertTrue(self.errors())
        self.setUp()
        self.data['surfaces'].pop()
        self.assertTrue(self.errors())

    def test_identity_ledger_reserves_identical_bytes_only(self):
        deployment.reserve(self.data, self.ledger)
        deployment.reserve(self.data, self.ledger)
        ledger = deployment.load_ledger(self.ledger)
        self.assertEqual(len(ledger['entries']), 1)
        self.assertEqual(self.errors(), [])
        self.data['targets'][0]['capabilities'] = ['different capability']
        self.assertTrue(self.errors())
        with self.assertRaises(ValueError):
            deployment.reserve(self.data, self.ledger)
        self.assertEqual(deployment.load_ledger(self.ledger), ledger)

    def test_reservation_lock_fails_closed(self):
        self.ledger.with_suffix('.json.lock').write_text('held')
        with self.assertRaises(FileExistsError):
            deployment.reserve(self.data, self.ledger)

    def test_clean_checkout_and_exact_revision_are_checked(self):
        source = self.root / 'source'
        source.mkdir()
        def git(*args):
            return subprocess.check_output(['git', '-C', str(source), *args], text=True).strip()
        git('init', '--quiet')
        git('-c', 'user.name=Synthetic Test', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '--quiet', '--allow-empty', '-m', 'Synthetic source')
        self.data['source']['revision'] = git('rev-parse', 'HEAD')
        deployment.verify_source(self.data, source)
        (source / 'unreviewed.txt').write_text('dirty')
        with self.assertRaises(ValueError):
            deployment.verify_source(self.data, source)
        self.data['source']['revision'] = self.sha
        with self.assertRaises(ValueError):
            deployment.verify_source(self.data, source)

    def test_website_archive_binds_exact_rendered_bytes(self):
        site = self.root / 'rendered'
        site.mkdir()
        (site / 'index.html').write_text('SYNTHETIC WEBSITE')
        archive = self.root / 'site.zip'
        with zipfile.ZipFile(archive, 'w') as z:
            z.write(site / 'index.html', 'index.html')
        tree = deployment.tree_digest(site)
        self.assertEqual(deployment.site_archive_digest(archive), tree)
        self.data.update(kind='website', channel='website', targets=[], site_tree_sha256=tree,
                         artifacts=[{'path': 'site.zip', 'sha256': deployment.digest(archive), 'size': archive.stat().st_size, 'role': 'site'}])
        self.assertEqual(self.errors(), [])
        (site / 'index.html').write_text('CHANGED WEBSITE')
        self.data['site_tree_sha256'] = deployment.tree_digest(site)
        self.assertTrue(self.errors())

    def test_website_archive_rejects_traversal_and_symlinks(self):
        archive = self.root / 'site.zip'
        with zipfile.ZipFile(archive, 'w') as z:
            z.writestr('../outside', 'hostile')
        with self.assertRaises(ValueError):
            deployment.site_archive_digest(archive)
        site = self.root / 'rendered'
        site.mkdir()
        (site / 'external').symlink_to('/etc/hosts')
        with self.assertRaises(ValueError):
            deployment.tree_digest(site)


class PublisherTests(DeploymentFixture, unittest.TestCase):
    def test_missing_password_proof_blocks_before_any_github_call(self):
        del self.data['targets'][0]['security_access']
        with patch.object(publish, 'gh') as gh, patch.object(publish, 'api_optional') as api:
            with self.assertRaises(ValueError):
                publish.publish(self.data, self.root, self.root, self.ledger, True)
            gh.assert_not_called()
            api.assert_not_called()

    def remote(self, draft=True):
        return {'draft': draft, 'assets': [{'name': Path(a['path']).name} for a in self.data['artifacts']],
                'tag_name': 'v' + self.version, 'target_commitish': self.sha,
                'body': publish.release_notes(self.data, self.root), 'name': self.data['release']['title'],
                'prerelease': True, 'html_url': 'https://github.com/example/synthetic/releases/tag/v' + self.version}

    def test_untagged_draft_is_found_in_paginated_release_list(self):
        draft = self.remote()
        pages = [[{'tag_name': 'unrelated'}], [draft]]
        with patch.object(publish, 'api_optional', return_value=None), patch.object(publish, 'gh', return_value=json.dumps(pages)) as gh:
            self.assertEqual(publish.lookup_release('example/synthetic', 'v' + self.version), draft)
            gh.assert_called_once_with('api', '--paginate', '--slurp', 'repos/example/synthetic/releases?per_page=100')

    def test_ambiguous_or_malformed_release_listing_fails_closed(self):
        for pages in ([[self.remote(), self.remote()]], {'not': 'pages'}):
            with self.subTest(pages=pages), patch.object(publish, 'api_optional', return_value=None), patch.object(publish, 'gh', return_value=json.dumps(pages)):
                with self.assertRaises(ValueError):
                    publish.lookup_release('example/synthetic', 'v' + self.version)

    def test_existing_release_avoids_fallback_listing(self):
        with patch.object(publish, 'api_optional', return_value=self.remote()), patch.object(publish, 'gh') as gh:
            self.assertEqual(publish.lookup_release('example/synthetic', 'v' + self.version), self.remote())
            gh.assert_not_called()

    def test_listed_draft_wrong_target_or_tag_blocks_before_download(self):
        for key, value in [('target_commitish', 'b' * 40), ('tag_name', 'different-tag')]:
            remote = self.remote()
            remote[key] = value
            with self.subTest(key=key), patch.object(publish, 'gh') as gh:
                with self.assertRaises(ValueError):
                    publish.verify_draft(self.data, self.root, remote, 'example/synthetic', 'v' + self.version)
                gh.assert_not_called()

    def test_incomplete_record_never_calls_github(self):
        with patch.object(publish, 'gh') as gh, patch.object(publish, 'api_optional') as api:
            with self.assertRaises(ValueError):
                publish.publish(deployment.template(), self.root, self.root, self.ledger)
            gh.assert_not_called()
            api.assert_not_called()

    def test_published_release_is_never_edited(self):
        with patch.object(publish, 'verify_source'), patch.object(publish, 'verify_tag'), patch.object(publish, 'verify_remote_source'), patch.object(publish, 'api_optional', return_value=self.remote(False)), patch.object(publish, 'gh') as gh:
            with self.assertRaises(ValueError):
                publish.publish(self.data, self.root, self.root, self.ledger, True)
            gh.assert_not_called()

    def test_draft_downloads_are_checked_not_just_upload_results(self):
        remote = self.remote()
        def wrong_download(*args):
            directory = Path(args[args.index('--dir') + 1])
            for asset in self.data['artifacts']:
                (directory / Path(asset['path']).name).write_bytes(b'WRONG REMOTE BYTES')
        with patch.object(publish, 'gh', side_effect=wrong_download):
            with self.assertRaises(ValueError):
                publish.verify_draft(self.data, self.root, remote, 'example/synthetic', 'v' + self.version)

    def test_draft_first_promotion_and_completion_remain_separate(self):
        remote = self.remote()
        with patch.object(publish, 'verify_source'), patch.object(publish, 'verify_tag'), patch.object(publish, 'verify_remote_source'), patch.object(publish, 'verify_draft'), patch.object(publish, 'api_optional', return_value=remote), patch.object(publish, 'gh') as gh:
            result = publish.publish(self.data, self.root, self.root, self.ledger)
            self.assertEqual(result['state'], 'verified_draft')
            gh.assert_not_called()
        with patch.object(publish, 'verify_source'), patch.object(publish, 'verify_tag'), patch.object(publish, 'verify_remote_source'), patch.object(publish, 'verify_draft') as verify, patch.object(publish, 'api_optional', side_effect=[remote, remote, self.remote(False)]), patch.object(publish, 'gh') as gh:
            result = publish.publish(self.data, self.root, self.root, self.ledger, True)
            self.assertEqual(result['state'], 'published_pending_live_surface_verification')
            self.assertEqual(verify.call_count, 3)
            gh.assert_called_once_with('release', 'edit', 'v' + self.version, '--repo', 'example/synthetic', '--draft=false', '--latest=false')

    def test_wrong_remote_tag_blocks_publication(self):
        with patch.object(publish, 'api_optional', return_value={'object': {'type': 'commit', 'sha': 'b' * 40}}):
            with self.assertRaises(ValueError):
                publish.verify_tag('example/synthetic', 'v' + self.version, self.sha)

    def test_remote_draft_reservation_survives_an_interrupted_upload(self):
        empty = self.remote()
        empty['assets'] = []
        with patch.object(publish, 'gh') as gh:
            publish.verify_draft(self.data, self.root, empty, 'example/synthetic', 'v' + self.version, allow_partial=True)
            gh.assert_not_called()
            self.data['targets'][0]['capabilities'] = ['changed attempt']
            with self.assertRaises(ValueError):
                publish.verify_draft(self.data, self.root, empty, 'example/synthetic', 'v' + self.version, allow_partial=True)

    def test_interrupted_upload_adds_only_missing_assets(self):
        partial, full = self.remote(), self.remote()
        partial['assets'] = partial['assets'][:1]
        with patch.object(publish, 'verify_source'), patch.object(publish, 'verify_tag'), patch.object(publish, 'verify_remote_source'), patch.object(publish, 'verify_draft'), patch.object(publish, 'api_optional', side_effect=[partial, full]), patch.object(publish, 'gh') as gh:
            result = publish.publish(self.data, self.root, self.root, self.ledger)
            self.assertEqual(result['state'], 'verified_draft')
            args = gh.call_args.args
            self.assertEqual(args[:3], ('release', 'upload', 'v' + self.version))
            self.assertNotIn('--clobber', args)
            self.assertNotIn(str((self.root / self.data['artifacts'][0]['path']).resolve()), args)
            self.assertIn(str((self.root / self.data['artifacts'][1]['path']).resolve()), args)

    def test_missing_remote_matching_source_blocks_before_mutation(self):
        with patch.object(publish, 'api_optional', return_value=None):
            with self.assertRaises(ValueError):
                publish.verify_remote_source('example/synthetic', self.sha)

    def test_cli_retains_a_blocked_receipt_without_remote_calls(self):
        record = self.root / 'deployment.json'
        record.write_text(json.dumps(deployment.template()))
        receipt = self.root / 'publication.json'
        args = ['publish.py', '--record', str(record), '--bundle', str(self.root),
                '--source-root', str(self.root), '--receipt', str(receipt)]
        with patch('sys.argv', args), patch.object(publish, 'gh') as gh, patch.object(publish, 'api_optional') as api, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(publish.main(), 1)
            gh.assert_not_called()
            api.assert_not_called()
        self.assertEqual(json.loads(receipt.read_text())['state'], 'blocked_or_uncertain')


if __name__ == '__main__':
    unittest.main()
