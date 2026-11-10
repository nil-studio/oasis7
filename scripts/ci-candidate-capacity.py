#!/usr/bin/env python3
"""Candidate-only hosted SDK reclamation; thresholds require measured tuning."""
import argparse,json,os,pathlib,re,shutil,subprocess,sys
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
def snapshot(paths):
    v=os.statvfs("/")
    items=[]
    for p in paths:
        row={"path":str(p),"exists":p.exists()}
        if p.exists() and not p.is_symlink():
            r=subprocess.run(["du","-sk",str(p)],text=True,capture_output=True)
            row["du_exit"]=r.returncode
            row["kib"]=int(r.stdout.split()[0]) if r.returncode==0 else None
        items.append(row)
    return {"free_bytes":v.f_bavail*v.f_frsize,"free_inodes":v.f_favail,"items":items}
def enough(after,min_bytes,min_inodes):
    if after["free_bytes"]<min_bytes or after["free_inodes"]<min_inodes: raise RuntimeError("candidate capacity below configured measurement threshold")
def reclaim(paths,apply=False,min_bytes=0,min_inodes=0,mountpoints=()):
    error=None
    try:
        validate(paths,mountpoints) # complete validation before any mutation
        print(json.dumps({"phase":"before","capacity":snapshot(paths)}),flush=True)
        for p in paths:
            present=p.exists()
            if present and apply: shutil.rmtree(p)
            print(json.dumps({"phase":"sdk","path":str(p),"result":"deleted" if present and apply else "present_probe" if present else "absent"}),flush=True)
    except BaseException as e:
        error=e
    finally:
        try:
            after=snapshot(paths)
            print(json.dumps({"phase":"after","capacity":after,"prior_error":error is not None}),flush=True)
            if error is None: enough(after,min_bytes,min_inodes)
        except BaseException:
            if error is None: raise
    if error is not None: raise error

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
    a=argparse.ArgumentParser();a.add_argument("--apply",action="store_true");a.add_argument("--min-free-bytes",type=int,required=True);a.add_argument("--min-free-inodes",type=int,required=True);v=a.parse_args()
    if v.min_free_bytes<=0 or v.min_free_inodes<=0: raise RuntimeError("positive explicit measurement thresholds required")
    release=dict(line.split("=",1) for line in pathlib.Path("/etc/os-release").read_text().splitlines() if "=" in line);release={k:x.strip(chr(34)) for k,x in release.items()}
    check_environment(os.environ,release,os.geteuid())
    capacity_domains(os.environ)
    reclaim([pathlib.Path(p) for p in ALLOWLIST],v.apply,v.min_free_bytes,v.min_free_inodes,mounts(pathlib.Path("/proc/self/mountinfo").read_text()))
if __name__=="__main__":main()
