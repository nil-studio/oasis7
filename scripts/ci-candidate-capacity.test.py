import importlib.util,pathlib,tempfile,unittest
from types import SimpleNamespace
from unittest.mock import patch
s=importlib.util.spec_from_file_location("capacity",pathlib.Path(__file__).with_name("ci-candidate-capacity.py"));m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
class Tests(unittest.TestCase):
 def test_reclaim_only_explicit_dirs(self):
  with tempfile.TemporaryDirectory() as t:
   r=pathlib.Path(t).resolve(); sdk=r/"sdk";sdk.mkdir();(sdk/"x").write_text("unused");keep=r/"target";keep.mkdir();m.reclaim([sdk],True);self.assertFalse(sdk.exists());self.assertTrue(keep.exists())
 def test_validate_all_before_delete(self):
  with tempfile.TemporaryDirectory() as t:
   r=pathlib.Path(t).resolve();sdk=r/"sdk";sdk.mkdir();link=r/"link";link.symlink_to(sdk,target_is_directory=True)
   with self.assertRaises(RuntimeError):m.reclaim([sdk,link],True)
   self.assertTrue(sdk.exists())
 def test_probe_does_not_delete(self):
  with tempfile.TemporaryDirectory() as t:
   p=pathlib.Path(t).resolve();m.reclaim([p]);self.assertTrue(p.exists())
 def test_mount_tree_rejected_before_delete(self):
  with tempfile.TemporaryDirectory() as t:
   p=pathlib.Path(t).resolve();sdk=p/"sdk";sdk.mkdir()
   with self.assertRaises(RuntimeError):m.reclaim([sdk],True,mountpoints=[sdk/"nested"])
   self.assertTrue(sdk.exists())
 def test_threshold_rejects(self):
  with self.assertRaises(RuntimeError):m.enough({"free_bytes":10,"free_inodes":9},11,1)
  with self.assertRaises(RuntimeError):m.enough({"free_bytes":10,"free_inodes":9},1,10)
 def test_delete_error_preserved(self):
  with tempfile.TemporaryDirectory() as t:
   with patch.object(m.shutil,"rmtree",side_effect=PermissionError("test")):
    with self.assertRaises(PermissionError):m.reclaim([pathlib.Path(t).resolve()],True)
 def test_mount_escape(self):
  self.assertEqual(m.mounts("1 2 3:4 / /sdk/a\\040b rw - ext4 dev rw"),[pathlib.Path("/sdk/a b")])
 def test_environment_rejects_other_job(self):
  with self.assertRaises(RuntimeError):m.check_environment({}, {"ID":"ubuntu","VERSION_ID":"24.04"},0)
 def test_same_device_domains(self):
  env={"GITHUB_WORKSPACE":"/work","RUNNER_TEMP":"/runner-temp"}
  capacity=SimpleNamespace(f_bavail=10,f_frsize=4096,f_favail=8)
  with patch.object(m.os,"statvfs",return_value=capacity), patch.object(m.pathlib.Path,"stat",return_value=SimpleNamespace(st_dev=7)) as stat:
   m.capacity_domains(env)
  self.assertEqual(stat.call_count,3)
 def test_different_device_domains_rejected(self):
  env={"GITHUB_WORKSPACE":"/work","RUNNER_TEMP":"/runner-temp"}
  capacity=SimpleNamespace(f_bavail=10,f_frsize=4096,f_favail=8)
  devices=[SimpleNamespace(st_dev=device) for device in (7,8,7)]
  with patch.object(m.os,"statvfs",return_value=capacity), patch.object(m.pathlib.Path,"stat",side_effect=devices):
   with self.assertRaisesRegex(RuntimeError,"different build filesystem"):
    m.capacity_domains(env)
 def test_reclaim_skips_du_by_default(self):
  with tempfile.TemporaryDirectory() as t:
   sdk=pathlib.Path(t).resolve()/"sdk";sdk.mkdir();(sdk/"x").write_text("unused")
   with patch.object(m.subprocess,"run",side_effect=AssertionError("du should not run by default")):
    m.reclaim([sdk],True)
   self.assertFalse(sdk.exists())
 def test_async_worker_and_wait_roundtrip(self):
  with tempfile.TemporaryDirectory() as t:
   r=pathlib.Path(t).resolve();sdk=r/"sdk";sdk.mkdir();(sdk/"x").write_text("unused")
   state=r/"sdk-reclaim.json"
   with patch.object(m.subprocess,"Popen") as popen:
    popen.return_value.pid=12345
    info=m.start_reclaim(state,[sdk],True,min_bytes=1,min_inodes=1)
    self.assertEqual(info["pid"],12345)
    self.assertTrue(popen.called)
   m.run_worker(state,[sdk],True,min_bytes=1,min_inodes=1)
   self.assertFalse(sdk.exists())
   with patch.object(m,"_pid_alive",return_value=False):
    m.wait_reclaim(state,[sdk],timeout_seconds=5)
 def test_wait_propagates_worker_failure_and_missing_status(self):
  with tempfile.TemporaryDirectory() as t:
   r=pathlib.Path(t).resolve();state=r/"sdk-reclaim.json"
   status=state.with_suffix(".status.json");log=state.with_suffix(".log")
   log.write_text("log\n")
   state.write_text('{"pid":999999,"log_path":"%s","status_path":"%s","min_free_bytes":1,"min_free_inodes":1}'%(log,status))
   with patch.object(m,"_pid_alive",return_value=False):
    with self.assertRaisesRegex(RuntimeError,"exited without status"):
     m.wait_reclaim(state,[],timeout_seconds=1)
   status.write_text('{"done":true,"ok":false,"error":"PermissionError: denied"}')
   with self.assertRaisesRegex(RuntimeError,"PermissionError: denied"):
    m.wait_reclaim(state,[],timeout_seconds=1)
if __name__=="__main__":unittest.main()
