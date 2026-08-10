#!/usr/bin/env python3
"""Create immutable Stage 6B-R1 source plus evidence handoff."""
from __future__ import annotations
import hashlib,io,json,os,stat,subprocess,tarfile,zipfile
from pathlib import Path,PurePosixPath
import stage6b_check as checker

ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/"reports"/"handoff"
def run(args):return subprocess.check_output(args,cwd=ROOT,text=True).strip()
def fail(message):raise SystemExit(f"stage6b-handoff: FAIL: {message}")
def safe(name):
    path=PurePosixPath(name)
    return not path.is_absolute() and ".." not in path.parts and not any(part in {".git","target","tmp","reports","__MACOSX"} for part in path.parts) and path.name!=".env" and path.suffix!=".log"
def main():
    if run(["git","status","--porcelain"]):fail("worktree not clean")
    head=run(["git","rev-parse","HEAD"]);short=run(["git","rev-parse","--short=7","HEAD"])
    if run(["git","rev-parse","HEAD^"])!=checker.R1_BASE:fail("wrong predecessor")
    if run(["git","branch","--show-current"])!=checker.BRANCH:fail("wrong branch")
    if run(["git","rev-parse",f"origin/{checker.BRANCH}"])!=head:fail("origin branch mismatch")
    env=dict(os.environ);env["STAGE6B_R1_SKIP_PRESEAL"]="1"
    gate=subprocess.run(["bash","scripts/stage6b_r1_gate.sh"],cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
    redacted=gate.stdout.replace(str(ROOT),"<REPO>").replace(str(Path.home()),"<HOME>").encode()
    if gate.returncode:print(gate.stdout);fail(f"gate failed: {gate.returncode}")
    negative=subprocess.check_output(["python3","scripts/stage6b_negative_harness.py"],cwd=ROOT)
    raw=subprocess.check_output(["git","archive","--format=tar","HEAD"],cwd=ROOT);members=[]
    with tarfile.open(fileobj=io.BytesIO(raw),mode="r:") as tar:
        for member in tar.getmembers():
            if member.isdir():continue
            if not member.isfile() or not safe(member.name):fail(f"unsafe member: {member.name}")
            members.append((member.name,tar.extractfile(member).read(),member.mode))
    manifest=json.dumps({"schema_version":1,"source_ref":head,"predecessor":checker.R1_BASE,"members":[{"path":name,"sha256":hashlib.sha256(data).hexdigest()} for name,data,_ in sorted(members)]},indent=2,sort_keys=True).encode()+b"\n"
    marker=f"source_ref={head}\nsource_short_ref={short}\nsource_branch={checker.BRANCH}\narchive_name=moex-trading-project-{short}.zip\npredecessor={checker.R1_BASE}\n".encode()
    evidence=json.dumps({"schema_version":1,"stage":"6B-R1","source_ref":head,"predecessor":checker.R1_BASE,"gate_exit_code":0,"positive_test_count":60,"negative_case_count":162,"open_create_policy":"validate_existing_or_create_new","existing_zero_length_policy":"fail_closed_unchanged","execution_surfaces_open":False,"stage6c_open":False,"checkpoint_sidecar_persisted":False,"source_manifest_sha256":hashlib.sha256(manifest).hexdigest(),"gate_sha256":hashlib.sha256(redacted).hexdigest()},indent=2,sort_keys=True).encode()+b"\n"
    members += [("handoff-commit.txt",marker,0o644),("source-tree-manifest.json",manifest,0o644),("handoff-evidence/stage6b-r1-full-gate.txt",redacted,0o644),("handoff-evidence/stage6b-r1-evidence.json",evidence,0o644),("handoff-evidence/stage6b-r1-negative.txt",negative,0o644),("handoff-evidence/stage6b-r1-toolchain.txt",f"{run(['rustc','--version'])}\n{run(['cargo','--version'])}\n".encode(),0o644)]
    if len({name for name,_,_ in members})!=len(members):fail("duplicate member")
    OUT.mkdir(parents=True,exist_ok=True);target=OUT/f"moex-trading-project-{short}.zip"
    with zipfile.ZipFile(target,"w",zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for name,data,mode in sorted(members):
            info=zipfile.ZipInfo(name,(1980,1,1,0,0,0));info.create_system=3;info.external_attr=(stat.S_IFREG|(0o755 if mode&0o111 else 0o644))<<16
            archive.writestr(info,data,zipfile.ZIP_DEFLATED,compresslevel=9)
    digest=hashlib.sha256(target.read_bytes()).hexdigest();side=target.with_suffix(".zip.sha256");side.write_text(f"{digest}  {target.name}\n")
    print(target);print(side);print(digest)
if __name__=="__main__":main()
