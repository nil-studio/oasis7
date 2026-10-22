#!/usr/bin/env python3
"""Bounded isolated extractor cargo-check only; never execute the extractor."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import time
import codeql_rust_diagnostic_runtime as runtime

SOURCE = 'a1575b98a8cc19bcfa7bb6cb41be688cb3dae65c'
LOCK_SHA = 'f89e3f85d9df14bed8d99d5c6a46e08c7243e0cbb14dd6f73904d346bb7b2111'
MODULE_SHA = '0742bcd492471324758a217292bd7206d2623d6af3548c77cb5f2a68ea4d48f5'
TOOLCHAIN = 'nightly-2026-09-03'
MEMBERS = ['shared/tree-sitter-extractor','shared/yeast','shared/yeast-macros','shared/yeast-schema',
           'ruby/extractor','unified/extractor','unified/swift-syntax-rs','rust/extractor',
           'rust/extractor/macros','rust/ast-generator','rust/autobuild']
DATA_LIMIT = 8*1024**3


class LeafRace(RuntimeError):
    pass


def owned_data_guard(work):
    root = work.lstat()
    if not stat.S_ISDIR(root.st_mode):
        raise RuntimeError('owned root is not a physical directory')
    identity = (root.st_dev, root.st_ino)
    for attempt in range(2):
        total = count = 0
        try:
            def walk_error(error):
                raise error  # directory/permission uncertainty is not a leaf race
            for folder, dirs, files in os.walk(work, followlinks=False, onerror=walk_error):
                for name in dirs + files:
                    path = Path(folder)/name
                    try:
                        value = path.lstat()
                    except FileNotFoundError as exc:
                        raise LeafRace('active data leaf vanished') from exc
                    if stat.S_ISLNK(value.st_mode):
                        total += value.st_blocks*512  # never follow a symlink
                    elif stat.S_ISDIR(value.st_mode) or stat.S_ISREG(value.st_mode):
                        total += value.st_blocks*512
                    else:
                        raise RuntimeError('unknown owned data file kind')
                    count += 1
                    if count > 400_000 or total > DATA_LIMIT:
                        raise RuntimeError('owned data observation bound exceeded')
            fresh = work.lstat()
            if (fresh.st_dev,fresh.st_ino) != identity or not stat.S_ISDIR(fresh.st_mode):
                raise RuntimeError('owned root identity changed')
            if shutil.disk_usage(work).free < 2*1024**3:
                raise RuntimeError('fresh free-space safety floor exhausted')
            return {'observed_allocated_bytes':total,'entries':count,'rescans':attempt,
                'policy_limit_bytes':DATA_LIMIT,'kind':'PERIODIC_OBSERVATION_NOT_OS_HARD_QUOTA'}
        except LeafRace:
            fresh = work.lstat()
            if (fresh.st_dev,fresh.st_ino) != identity:
                raise RuntimeError('owned root identity changed')
            if attempt:
                raise RuntimeError('owned data scan remains INCOMPLETE after one rescan')
    raise AssertionError('unreachable')


def source_snapshot(source):
    result = {}
    for name in MEMBERS:
        root = source/name
        for path in root.rglob('*'):
            if path.is_file() and (path.suffix == '.rs' or path.name in ('Cargo.toml','Cargo.lock')):
                result[str(path.relative_to(source))] = runtime.digest(path)
    for name in ('Cargo.toml','Cargo.lock','MODULE.bazel','rust-toolchain.toml'):
        result[name] = runtime.digest(source/name)
    return result


def execute(args):
    remaining = args.deadline-time.time()
    if not 0 < remaining <= 1080:
        raise RuntimeError('invalid fixed first-preparation deadline')
    end = time.monotonic()+remaining
    out, work = args.output, args.work
    out.mkdir(exist_ok=False); work.mkdir(exist_ok=False)
    if shutil.disk_usage(work).free < 10*1024**3:
        raise RuntimeError('NOT_REACHED fresh space below10GiB')
    env = {k:v for k,v in os.environ.items() if k not in
           ('GH_TOKEN','GITHUB_TOKEN','RUSTC_WRAPPER','CARGO_TARGET_DIR','CARGO_HOME','RUSTUP_HOME','RUSTFLAGS')}
    cargo = work/'cargo-home'; target = work/'target'; rustup = work/'rustup-home'
    cargo.mkdir(); target.mkdir(); rustup.mkdir()
    env.update(CARGO_HOME=str(cargo.resolve()),CARGO_TARGET_DIR=str(target.resolve()),
        RUSTUP_TOOLCHAIN=TOOLCHAIN,RUSTUP_HOME=str(rustup.resolve()),CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',
        CARGO_BUILD_JOBS='2',OASIS7_RUST_INVENTORY='0')
    terminal = {'status':'FAILED','source':SOURCE,'toolchain':TOOLCHAIN,
        'extractor_executions':0,'test_binary_executions':0,'extractions':0,'query_evaluations':0,
        'release_equivalence':'NOT_CLAIMED','full_rust_coverage':'HOLD'}
    source = work/'source'; source.mkdir()
    before = None
    primary = None
    def run(argv,name,timeout,cwd=source,env_override=None):
        return runtime.run(argv,cwd,out/(name+'.log'),end-90,timeout,out,env if env_override is None else env_override,
            file_limit=runtime.DATA_LIMIT,safety_check=lambda:owned_data_guard(work))
    try:
        setup_end = min(end-90,time.monotonic()+120)
        for argv,name in [(['git','init','--quiet'],'init'),
            (['git','remote','add','origin','https://github.com/github/codeql.git'],'remote'),
            (['git','config','gc.auto','0'],'disable-gc'),
            (['git','config','maintenance.auto','false'],'disable-maintenance'),
            (['git','config','core.sparseCheckout','true'],'sparse-config'),
            (['git','fetch','--filter=blob:none','--depth=1','origin',SOURCE],'fetch'),
            (['git','sparse-checkout','set','--cone',*MEMBERS],'sparse'),
            (['git','checkout','--detach',SOURCE],'checkout')]:
            run(argv,name,max(0,setup_end-time.monotonic()))
        run(['git','rev-parse','HEAD'],'source-head',10)
        if (out/'source-head.log').read_text().strip() != SOURCE:
            raise RuntimeError('source checkout identity differs')
        if runtime.digest(source/'Cargo.lock') != LOCK_SHA or runtime.digest(source/'MODULE.bazel') != MODULE_SHA:
            raise RuntimeError('official workspace lock/build toolchain identity differs')
        action=getattr(args,'action','check')
        observation=business=None
        if action=='observe':
            import codeql_rust_inventory_observe as observation
            business=observation.prepare_business(run,work,setup_end)
            run(['git','show',SOURCE+':rust/ql/lib/rust.dbscheme'],'source-schema',max(0,setup_end-time.monotonic()))
        runtime.save(out/'build-inputs.json',{'source':SOURCE,'lock_sha256':LOCK_SHA,'module_sha256':MODULE_SHA,
            'toolchain':TOOLCHAIN,'cargo_package':'codeql-rust','workspace_members':MEMBERS,
            'action':action,'check_jobs':2,'debug_info':0,'cargo_memory_hint':'UNSUPPORTED; sampled RSS is observation, no hard cap','source_fetch_budget_seconds':120,
            'toolchain_budget_seconds':120,'check_budget_seconds':720 if action=='check' else None,'binary_build_shared_seconds':360 if action=='observe' else None,'total_seconds':1080,
            'receipt_reserve_seconds':90,'data_limit': '8GiB periodic allocated observation;1GiB per file',
            'evidence_limit_bytes':runtime.CAP},out)
        toolchain_end=min(end-90,time.monotonic()+120)
        run(['rustup','toolchain','install',TOOLCHAIN,'--profile','minimal'],'install-toolchain',max(0,toolchain_end-time.monotonic()))
        run(['rustup','run',TOOLCHAIN,'rustc','-vV'],'rustc',10)
        (source/'.oasis7-owned-inventory-source').touch()
        spec=importlib.util.spec_from_file_location('overlay',Path(__file__).with_name('codeql-rust-inventory')/'apply.py')
        overlay=importlib.util.module_from_spec(spec);spec.loader.exec_module(overlay)
        if action=='observe':
            observation.execute(args,env,end,out,work,source,business,run,overlay,source_snapshot,
                lambda:owned_data_guard(work),terminal,toolchain_end)
            return
        runtime.save(out/'overlay.json',overlay.apply(source),out)
        before = source_snapshot(source)
        runtime.save(out/'terminal-size-admission.json',runtime.admit_terminal_snapshots(before,{}),out)
        runtime.save(out/'candidate-source-before.json',before,out)
        if end-time.monotonic() < 180:
            raise RuntimeError('NOT_REACHED insufficient fixed check window')
        run(['cargo','check','--locked','-p','codeql-rust','--tests','--jobs','2'],'cargo-check',720)
        if runtime.digest(source/'Cargo.lock') != LOCK_SHA:
            raise RuntimeError('official workspace lock changed')
        terminal['status']='CARGO_CHECK_ONLY_COMPLETED'
    except BaseException as exc:
        primary = exc
        terminal['error']=repr(exc)[:8192]
        raise
    finally:
        if before is not None:
            try:
                after = source_snapshot(source)
                runtime.save(out/'candidate-source-after.json',after,out,terminal=True)
                if after != before:
                    raise RuntimeError('candidate source changed during cargo check')
            except BaseException as verification_error:
                terminal['source_verification_error']=repr(verification_error)[:2048]
                terminal['status']='FAILED'
                if primary is None:
                    primary = verification_error
        runtime.save(out/'terminal.json',terminal,out,terminal=True)
        hashes={str(p.relative_to(out)):runtime.digest(p) for p in out.rglob('*') if p.is_file()}
        if len(hashes)>256 or len((json.dumps(hashes,indent=2)+'\n').encode())>runtime.MAX_HASH_RECEIPT:
            raise RuntimeError('check evidence hash index admission failed')
        runtime.save(out/'evidence-hashes.json',hashes,out,terminal=True)
        if primary is not None and terminal.get('source_verification_error') and terminal.get('error') is None:
            raise primary


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--action',choices=('check','observe'),default='check')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--work',type=Path,required=True)
    parser.add_argument('--deadline',type=float,required=True)
    execute(parser.parse_args())
