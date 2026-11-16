#!/usr/bin/env python3
"""Pure restart orchestration regressions; no service or network operations."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import Mock, patch
spec=importlib.util.spec_from_file_location('smoke',Path(__file__).with_name('package-ecs-cold-entry-smoke.py'))
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
def commit(position):
    return {'world':{'world_id':'world','genesis_digest':'genesis'},'binding':{'branch_id':'branch'},
            'position':position,'execution_block_hash':'block-'+str(position),'state_root_ref':'state-'+str(position)}
class Tests(unittest.TestCase):
    def test_full_commit_history_and_same_height_match(self):
        m.validate_restart_commits(commit(1),commit(2));m.validate_restart_commits(commit(1),commit(1))
        for field,value in [('position',True),('world',{'world_id':'other'}),('binding',{'branch_id':'other'}),('execution_block_hash','other')]:
            bad=commit(1);bad[field]=value
            with self.subTest(field=field):
                with self.assertRaises(RuntimeError):m.validate_restart_commits(commit(1),bad)
        with self.assertRaises(RuntimeError):m.validate_restart_commits(commit(2),commit(1))
    def test_actual_restart_uses_original_argv_and_retries_fresh_signed_owner_read(self):
        with tempfile.TemporaryDirectory() as temp:
            smoke=Mock();smoke.root=Path(temp);smoke.deadline=time.monotonic()+100;smoke.remaining.return_value=100
            original=Mock();original.pid=123;original.poll.return_value=None
            restarted=Mock();restarted.poll.return_value=None;smoke.children=[original]
            plan={'orchestration_directory':'private-vault','entry':{'world':{'world_id':'world','genesis_digest':'genesis'},'trusted_world_service_key':'service','identity_instance_epoch':'1','identity':{'identity_service_id':'identity','environment_id':'isolated'}}}
            argv=['chain','--world-id','world','--keep','original']
            original.args=argv;restarted.args=argv;smoke.env={'trusted':'same'};smoke.server_environment=dict(smoke.env);smoke.expand.side_effect=lambda value:value
            fixture={'restart_committed_chain':True,'client_binary':'client','server_commands':[argv,['launcher']]}
            status={'participant_operation_id':'p'*43,'active_login_operation_id':'l'*43}
            observation={'version':{'commit':commit(4)},'receipt':{'original':'participant'},'owner':{'original':'owner'},'task':{'original':'task'}}
            latest=copy.deepcopy(observation);latest['version']['commit']=commit(5)
            commands=[];reads=[]
            def command(args,private=False,start=False):
                commands.append((args,private,start))
                if start:return restarted
                if 'status' in args:return 0,json.dumps(status).encode()
                reads.append(args)
                if len(reads)==1:return 0,json.dumps(observation).encode()
                if len(reads)==2:return 1,b'unknown'
                return 0,json.dumps(latest).encode()
            smoke.command.side_effect=command
            discovery={**plan['entry'],'world_service_public_key':'service','identity':{'service_id':'identity','environment_id':'isolated','instance_epoch':'1'},'phase_capabilities':{p:True for p in ('machine_registration','participant_registration','owner_control','executor_control')}}
            with patch.object(m.os,'killpg') as kill,patch.object(m.time,'sleep'):
                proof=m.restart_committed_chain(smoke,fixture,original,plan,lambda *_:discovery)
            kill.assert_called_once_with(123,m.signal.SIGTERM);original.wait.assert_called_once()
            self.assertEqual([x[0] for x in commands if x[2]],[argv]);self.assertEqual(len(reads),3)
            self.assertEqual(reads[0],reads[1]);self.assertEqual(reads[1],reads[2])
            self.assertEqual(proof['before_position'],4);self.assertEqual(proof['after_position'],5)
            self.assertEqual((Path(temp)/'restart-owner-entry.json').stat().st_mode&0o777,0o600)
    def test_external_fixture_does_not_gain_restart_behavior(self):
        smoke=Mock();self.assertIsNone(m.restart_committed_chain(smoke,{},None,{}));smoke.command.assert_not_called()
    def test_standard_fixture_requires_restart(self):
        source=Path(__file__).with_name('package-ecs-cold-entry-fixture.py').read_text()
        self.assertIn('"restart_committed_chain": True',source)

fixture_spec=importlib.util.spec_from_file_location('fixture',Path(__file__).with_name('package-ecs-cold-entry-fixture.py'))
f=importlib.util.module_from_spec(fixture_spec);fixture_spec.loader.exec_module(f)
class FixtureAuthorityTests(unittest.TestCase):
    def smoke(self, root):
        smoke=Mock();smoke.root=Path(root);smoke.variables={'origin':'https://localhost:9443'}
        return smoke
    def test_default_and_public_authority_use_distinct_local_bootstrap(self):
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root)
            self.assertEqual(f.authority_binding(smoke), {'origin':'https://localhost:9443','rp_id':'localhost','bootstrap_tls_host':'localhost','bootstrap_tls_port':9443})
            public=f.authority_binding(smoke,public_origin='https://authority.example.test',rp_id='authority.example.test')
            self.assertEqual(public['origin'],'https://authority.example.test')
            self.assertEqual(public['rp_id'],'authority.example.test')
            self.assertEqual(public['bootstrap_tls_port'],9443)
            self.assertEqual(public['bootstrap_tls_host'],'localhost')
    def test_malformed_authority_is_rejected_before_world_setup(self):
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root)
            for origin in ['http://authority.example.test','https://authority.example.test/','https://authority.example.test/path','https://user@authority.example.test','https://authority.example.test?x=1','https://authority.example.test#x','https://authority.example.test:0','https://authority.example.test:443','https://authority.example.test:65536','https://127.0.0.1','https://AUTHORITY.example.test','https://authority.example.test\n']:
                with self.subTest(origin=origin),self.assertRaises(ValueError):
                    f.authority_binding(smoke,public_origin=origin,rp_id='authority.example.test')
            for rp in ['example.test','other.example.test','authority.example.test:443','https://authority.example.test','']:
                with self.subTest(rp=rp),self.assertRaises(ValueError):
                    f.authority_binding(smoke,public_origin='https://authority.example.test',rp_id=rp)
            for port in [0,65536,True,'9443',-1]:
                with self.subTest(port=port),self.assertRaises(ValueError):
                    f.authority_binding(smoke,bootstrap_tls_port=port)
    def test_binding_is_claimed_once_and_cannot_rebind_existing_authority(self):
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root);binding=f.authority_binding(smoke)
            f.claim_authority(smoke.root,binding)
            path=smoke.root/'fixture-authority.json';original=path.read_bytes()
            self.assertEqual(path.stat().st_mode&0o777,0o600)
            for next_binding in [binding,{**binding,'origin':'https://other.example.test','rp_id':'other.example.test'}]:
                with self.assertRaises(RuntimeError):f.claim_authority(smoke.root,next_binding)
            self.assertEqual(path.read_bytes(),original)
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root);(smoke.root/'preview').mkdir()
            with self.assertRaises(RuntimeError):f.claim_authority(smoke.root,f.authority_binding(smoke))
            self.assertFalse((smoke.root/'fixture-authority.json').exists())
    def test_bootstrap_uses_local_verified_tls_but_public_host(self):
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root);smoke.remaining.return_value=10
            binding=f.authority_binding(smoke,public_origin='https://authority.example.test',rp_id='authority.example.test')
            connection=Mock();connection.getresponse.return_value.status=200
            connection.getresponse.return_value.read.return_value=b'{"ok":true}'
            context=Mock()
            with patch.object(f.ssl,'create_default_context',return_value=context) as ca,patch.object(f.http.client,'HTTPSConnection',return_value=connection) as connect:
                f.wait_identity_bootstrap(smoke,binding)
            ca.assert_called_once_with(cafile=str(smoke.root/'root.crt'))
            connect.assert_called_once_with('localhost',9443,context=context,timeout=2)
            connection.request.assert_called_once_with('GET','/api/public/hosted-account/v2/capabilities',headers={'Host':'authority.example.test'})

    def test_tls_failure_does_not_follow_redirect_or_accept_untrusted_peer(self):
        with tempfile.TemporaryDirectory() as root:
            smoke=self.smoke(root);smoke.remaining.return_value=1
            binding=f.authority_binding(smoke)
            connection=Mock();connection.getresponse.return_value.status=302
            connection.getresponse.return_value.read.return_value=b'{"ok":true}'
            with patch.object(f.ssl,'create_default_context'),patch.object(f.http.client,'HTTPSConnection',return_value=connection),patch.object(f.time,'sleep'):
                with self.assertRaises(RuntimeError):f.wait_identity_bootstrap(smoke,binding)
            self.assertEqual(connection.request.call_count,60)
            for failure in [f.ssl.SSLCertVerificationError('wrong CA'),f.ssl.CertificateError('hostname mismatch')]:
                with patch.object(f.ssl,'create_default_context'),patch.object(f.http.client,'HTTPSConnection',side_effect=failure),patch.object(f.time,'sleep'):
                    with self.assertRaises(RuntimeError):f.wait_identity_bootstrap(smoke,binding)
    def test_actual_fixture_assembly_uses_public_binding_everywhere(self):
        for public in [False,True]:
            with self.subTest(public=public),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);smoke=self.smoke(root);smoke.env={'SSL_CERT_FILE':str(root/'root.crt')}
                smoke.fixture={'identity_upstream':['127.0.0.1',9000]};smoke.stage='setup'
                bundle=root/'bundle';(bundle/'bin').mkdir(parents=True)
                for name in ['oasis7_chain_runtime','oasis7_game_launcher','oasis7_game_client','oasis7_viewer_live']:(bundle/'bin'/name).touch()
                calls=[]
                def command(argv,private=False,start=False):
                    calls.append(list(argv))
                    if start:return Mock()
                    if argv[1]=='initialize-preview-world':
                        preview=root/'preview';(preview/'private').mkdir(parents=True)
                        (preview/'viewer.public.json').write_text(json.dumps({'world':{'world_id':'w','genesis_digest':'g'},'service_public_key':'s'}))
                        (preview/'private/viewer.env').write_text('')
                    if argv[0]=='openssl':Path(argv[argv.index('-out')+1]).write_bytes(b'x'*64)
                    if argv[1:3]==['devlocal-participant','inspect-head']:Path(argv[argv.index('--output')+1]).write_text('{}')
                    return 0,b''
                smoke.command.side_effect=command
                with self.assertRaises(ValueError):
                    f.fixture(smoke,bundle,str(bundle/'bin/oasis7_game_client'),public_origin='https://authority.example.test:443')
                smoke.command.assert_not_called()
                self.assertFalse((root/'fixture-authority.json').exists())
                options={'public_origin':'https://authority.example.test:8443','rp_id':'authority.example.test'} if public else {}
                with patch.object(f,'wait_identity_bootstrap') as wait,patch.object(f.os,'killpg'):
                    result=f.fixture(smoke,bundle,str(bundle/'bin/oasis7_game_client'),**options)
                origin=options.get('public_origin','https://localhost:9443');rp='authority.example.test' if public else 'localhost'
                self.assertEqual(smoke.env['OASIS7_HOSTED_WEBAUTHN_ORIGINS'],origin)
                self.assertEqual(smoke.env['OASIS7_HOSTED_WEBAUTHN_RP_ID'],rp)
                exported=json.loads((root/'export-request.json').read_text())
                self.assertEqual(exported['rp_config']['rp_id'],rp)
                self.assertEqual(exported['rp_config']['allowed_origins'],[origin])
                self.assertEqual(result['plan_template']['entry']['identity']['endpoint'],origin)
                self.assertEqual(result['plan_template']['entry']['identity']['audience'],origin)
                self.assertEqual(smoke.variables['origin'],origin)
                self.assertEqual(wait.call_args.args[1]['bootstrap_tls_port'],9443)
                if public:self.assertNotIn('SSL_CERT_FILE',smoke.env)
                else:self.assertEqual(smoke.env['SSL_CERT_FILE'],str(root/'root.crt'))
                self.assertIn('--devlocal-participant-policy-file',result['server_commands'][0])
                self.assertNotIn('--replication-network-listen',result['server_commands'][0])
                claim=json.loads((root/'fixture-authority.json').read_text());self.assertEqual(claim['origin'],origin)
                # Reusing this root must reject before even the first binary is called.
                smoke.command.reset_mock();smoke.variables['origin']='https://localhost:9443'
                with self.assertRaises(RuntimeError):f.fixture(smoke,bundle,str(bundle/'bin/oasis7_game_client'),public_origin='https://other.example.test')
                smoke.command.assert_not_called()
if __name__=='__main__':unittest.main()
