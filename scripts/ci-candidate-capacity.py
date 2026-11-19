#!/usr/bin/env python3
"""Candidate-only hosted SDK reclamation; thresholds require measured tuning."""
import argparse,json,os,pathlib,re,shutil,subprocess,sys,time
ALLOWLIST=("/usr/local/lib/android","/usr/share/dotnet","/opt/ghc")
def mounts(raw):
    def unescape(s): return re.sub(r"\\([0-7]{3})",lambda m:chr(int(m[1],8)),s)
    return [pathlib.Path(unescape(row.split()[4])) for row in raw.splitlines() if row.strip()]
def validate(paths,mountpoints):
    for path in paths:
        for component in [*reversed(path.parents),path]:
            if component.is_symlink(): raise RuntimeError("symlink in SDK path")
        if path.exists() and not path.is_dir(): raise RuntimeError("SDK path is not directory")
        if any(m==path or path in m.parents for m in mountpoints): raise RuntimeError("mount in SDK tree")
def snapshot(paths,measure_sizes=False):
    v=os.statvfs("/")
    items=[]
    for p in paths:
        row={"path":str(p),"exists":p.exists()}
        if measure_sizes and p.exists() and not p.is_symlink():
            r=subprocess.run(["du","-sk",str(p)],text=True,capture_output=True)
            row["du_exit"]=r.returncode
            row["kib"]=int(r.stdout.split()[0]) if r.returncode==0 else None
        items.append(row)
    return {"free_bytes":v.f_bavail*v.f_frsize,"free_inodes":v.f_favail,"items":items}
def enough(after,min_bytes,min_inodes):
    if after["free_bytes"]<min_bytes or after["free_inodes"]<min_inodes: raise RuntimeError("candidate capacity below configured measurement threshold")
def reclaim(paths,apply=False,min_bytes=0,min_inodes=0,mountpoints=(),measure_sizes=False):
    error=None
    try:
        validate(paths,mountpoints) # complete validation before any mutation
        print(json.dumps({"phase":"before","capacity":snapshot(paths,measure_sizes=measure_sizes)}),flush=True)
        for p in paths:
            present=p.exists()
            if present and apply: shutil.rmtree(p)
            print(json.dumps({"phase":"sdk","path":str(p),"result":"deleted" if present and apply else "present_probe" if present else "absent"}),flush=True)
    except BaseException as e:
        error=e
    finally:
        try:
            after=snapshot(paths,measure_sizes=measure_sizes)
            print(json.dumps({"phase":"after","capacity":after,"prior_error":error is not None}),flush=True)
            if error is None: enough(after,min_bytes,min_inodes)
        except BaseException:
            if error is None: raise
    if error is not None: raise error

def start_reclaim(state_file,paths,apply=False,min_bytes=0,min_inodes=0,mountpoints=(),measure_sizes=False):
    validate(paths,mountpoints)
    state_file=pathlib.Path(state_file)
    state_file.parent.mkdir(parents=True,exist_ok=True)
    log_path=state_file.with_suffix(".log")
    status_path=state_file.with_suffix(".status.json")
    for p in (state_file,log_path,status_path):
        if p.exists() or p.is_symlink(): p.unlink()
    cmd=[sys.executable,str(pathlib.Path(__file__).resolve()),"--worker",str(state_file),"--min-free-bytes",str(min_bytes),"--min-free-inodes",str(min_inodes)]
    if apply: cmd.append("--apply")
    if measure_sizes: cmd.append("--measure-sizes")
    with open(log_path,"wb") as log_fp:
        proc=subprocess.Popen(cmd,stdin=subprocess.DEVNULL,stdout=log_fp,stderr=subprocess.STDOUT,start_new_session=True,close_fds=True)
    state={"pid":proc.pid,"log_path":str(log_path),"status_path":str(status_path),"min_free_bytes":min_bytes,"min_free_inodes":min_inodes}
    state_file.write_text(json.dumps(state))
    print(json.dumps({"phase":"started","pid":proc.pid,"state_file":str(state_file)}),flush=True)
    return state

def run_worker(state_file,paths,apply=False,min_bytes=0,min_inodes=0,mountpoints=(),measure_sizes=False):
    status_path=pathlib.Path(state_file).with_suffix(".status.json")
    err=None
    try:
        reclaim(paths,apply,min_bytes,min_inodes,mountpoints,measure_sizes=measure_sizes)
    except BaseException as e:
        err=e
    tmp=status_path.with_suffix(".tmp")
    tmp.write_text(json.dumps({"done":True,"ok":err is None,"error":f"{type(err).__name__}: {err}" if err else None}))
    tmp.replace(status_path)
    if err is not None: raise err

def _pid_alive(pid):
    try:
        os.kill(pid,0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True

def wait_reclaim(state_file,paths,timeout_seconds=600):
    state=json.loads(pathlib.Path(state_file).read_text())
    status_path=pathlib.Path(state["status_path"])
    log_path=pathlib.Path(state["log_path"])
    pid=state["pid"]
    deadline=time.monotonic()+timeout_seconds
    while not status_path.exists():
        if not _pid_alive(pid):
            time.sleep(0.05)
            if status_path.exists(): break
            if log_path.exists(): sys.stdout.write(log_path.read_text()); sys.stdout.flush()
            raise RuntimeError(f"SDK reclaim worker {pid} exited without status")
        if time.monotonic()>=deadline:
            if log_path.exists(): sys.stdout.write(log_path.read_text()); sys.stdout.flush()
            raise RuntimeError(f"timed out waiting for SDK reclaim worker {pid}")
        time.sleep(0.2)
    if log_path.exists():
        sys.stdout.write(log_path.read_text())
        sys.stdout.flush()
    status=json.loads(status_path.read_text())
    if not status.get("ok"):
        raise RuntimeError(f"SDK reclaim worker failed: {status.get('error')}")
    after=snapshot(paths)
    print(json.dumps({"phase":"wait_verified","capacity":after}),flush=True)
    enough(after,state["min_free_bytes"],state["min_free_inodes"])

def check_environment(env,release,uid):
    if sys.platform!="linux" or env.get("GITHUB_ACTIONS")!="true" or env.get("RUNNER_ENVIRONMENT")!="github-hosted" or env.get("RUNNER_OS")!="Linux": raise RuntimeError("hosted Linux required")
    if release.get("ID")!="ubuntu" or release.get("VERSION_ID")!="24.04": raise RuntimeError("Ubuntu24 required")
    if env.get("GITHUB_JOB")!="oasis7-test-build-candidate" or env.get("OASIS7_CAPACITY_PURPOSE")!="ordinary-native-candidate-v1" or not env.get("GITHUB_WORKFLOW_REF","").split("@",1)[0].endswith("/.github/workflows/rust.yml"): raise RuntimeError("fixed candidate workflow required")
    if uid!=0: raise RuntimeError("explicit root required")
def capacity_domains(env):
    paths=[pathlib.Path("/"),pathlib.Path(env["GITHUB_WORKSPACE"]),pathlib.Path(env["RUNNER_TEMP"])]
    domains=[]
    for path in paths:
        v=os.statvfs(path)
        domains.append({"path":str(path),"device":path.stat().st_dev,"free_bytes":v.f_bavail*v.f_frsize,"free_inodes":v.f_favail})
    print(json.dumps({"phase":"capacity_domains","domains":domains}),flush=True)
    if len({v["device"] for v in domains})!=1: raise RuntimeError("SDK cleanup cannot serve a different build filesystem")
def main():
    a=argparse.ArgumentParser();a.add_argument("--apply",action="store_true");a.add_argument("--measure-sizes",action="store_true");a.add_argument("--min-free-bytes",type=int);a.add_argument("--min-free-inodes",type=int);a.add_argument("--start");a.add_argument("--worker");a.add_argument("--wait");a.add_argument("--wait-timeout-seconds",type=int,default=600);v=a.parse_args()
    release=dict(line.split("=",1) for line in pathlib.Path("/etc/os-release").read_text().splitlines() if "=" in line);release={k:x.strip(chr(34)) for k,x in release.items()}
    check_environment(os.environ,release,os.geteuid())
    paths=[pathlib.Path(p) for p in ALLOWLIST]
    if v.wait:
        wait_reclaim(v.wait,paths,timeout_seconds=v.wait_timeout_seconds)
        return
    if not v.min_free_bytes or not v.min_free_inodes or v.min_free_bytes<=0 or v.min_free_inodes<=0: raise RuntimeError("positive explicit measurement thresholds required")
    capacity_domains(os.environ)
    mountpoints=mounts(pathlib.Path("/proc/self/mountinfo").read_text())
    if v.start:
        start_reclaim(v.start,paths,v.apply,v.min_free_bytes,v.min_free_inodes,mountpoints,measure_sizes=v.measure_sizes)
    elif v.worker:
        run_worker(v.worker,paths,v.apply,v.min_free_bytes,v.min_free_inodes,mountpoints,measure_sizes=v.measure_sizes)
    else:
        reclaim(paths,v.apply,v.min_free_bytes,v.min_free_inodes,mountpoints,measure_sizes=v.measure_sizes)
if __name__=="__main__":main()
