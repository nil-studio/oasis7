import importlib.util,pathlib,tempfile,unittest
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
  with tempfile.TemporaryDirectory() as t:
   m.capacity_domains({"GITHUB_WORKSPACE":t,"RUNNER_TEMP":t})
 def test_different_device_domains_rejected(self):
  from types import SimpleNamespace
  with patch.object(m.pathlib.Path,"stat",side_effect=[SimpleNamespace(st_dev=1),SimpleNamespace(st_dev=2),SimpleNamespace(st_dev=1)]):
   with self.assertRaises(RuntimeError):m.capacity_domains({"GITHUB_WORKSPACE":"/","RUNNER_TEMP":"/"})
if __name__=="__main__":unittest.main()
