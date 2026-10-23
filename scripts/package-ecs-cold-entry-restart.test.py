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
if __name__=='__main__':unittest.main()
