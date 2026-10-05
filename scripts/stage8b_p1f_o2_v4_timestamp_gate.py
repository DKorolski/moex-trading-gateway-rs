#!/usr/bin/env python3
"""Exact-archive bounded postseal local gate, no operational IO or Git writes."""
import argparse
from pathlib import Path
import subprocess
import sys
import zipfile

import stage8b_p1f_nrg01_test_gate as bounded
import stage8b_p1f_o2_v4_timestamp_artifact as binding

a = binding.base


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('archive',type=Path)
    p.add_argument('output',type=Path)
    args=p.parse_args()
    archive, output=args.archive.resolve(),args.output.resolve()
    a.require(not a.source.builder.git('status','--porcelain').strip(),'clean commit required')
    ref=a.source.builder.git('rev-parse','HEAD').decode().strip()
    a.require(a.check(archive)['reviewed_ref']==ref,'archive/tree mismatch')
    output.mkdir(parents=True,exist_ok=False)
    before=bounded.inventory()
    digest=a.sha(archive.read_bytes())
    planned=[
        ([sys.executable,'-m','py_compile','scripts/stage8b_p1f_o2_v4_timestamp_build.py',
          'scripts/stage8b_p1f_o2_v4_timestamp_artifact.py','scripts/stage8b_p1f_o2_v4_timestamp_gate.py',
          'scripts/test_stage8b_p1f_o2_v4_timestamp_artifact.py'],30),
        (['bash','-n',a.SMOKE],30),
        (['rustfmt','--edition','2021','--check',a.PROBE],30),
        ([sys.executable,'scripts/current_tree_authority_check.py'],60),
        ([sys.executable,'scripts/current_tree_authority_negative_harness.py'],300),
        (['cargo','fmt','--all','--check'],120),
        (['cargo','clippy','--offline','--workspace','--all-targets','--all-features','--','-D','warnings'],1200),
        (['git','diff','--exit-code',a.source.SOURCE_REF,'HEAD','--',
          'crates','Cargo.toml','Cargo.lock','.github'],60),
        ([sys.executable,'scripts/stage8b_p1f_o2_v4_timestamp_artifact.py','check',str(archive)],120),
        ([sys.executable,'scripts/test_stage8b_p1f_o2_v4_timestamp_artifact.py',str(archive)],300),
        (['git','diff','--check'],30),
    ]
    records=[]
    for index,(command,seconds) in enumerate(planned):
        print('RUN '+' '.join(command),flush=True)
        record=bounded.run(command,output/f'{index:02d}',seconds)
        records.append(record)
        print(('PASS' if record['exit_code']==0 else 'FAIL')+' '+record['directory'],flush=True)
        if record['exit_code']:
            break
    unchanged=before==bounded.inventory() and digest==a.sha(archive.read_bytes())
    passed=unchanged and len(records)==len(planned) and all(r['exit_code']==0 for r in records)
    result=dict(source_ref=ref,archive_name=archive.name,archive_sha256=digest,
        source_inventory=before,source_and_archive_unchanged=unchanged,commands=records,
        planned_commands=len(planned),gate_passed=passed,github_ci_claimed=False,
        installation_authorized=False,execution_authorized=False)
    (output/'result.json').write_bytes(a.encoded(result))
    bundle=Path(str(archive)+'.local-gate.zip')
    with zipfile.ZipFile(bundle,'x',zipfile.ZIP_DEFLATED) as z:
        for path in sorted(output.rglob('*')):
            if path.is_file():
                z.writestr(a.common.zip_info(path.relative_to(output).as_posix(),'100644'),path.read_bytes())
    Path(str(bundle)+'.sha256').write_text(a.sha(bundle.read_bytes())+'  '+bundle.name+'\n')
    print('V4_TIMESTAMP_ARTIFACT_GATE='+('PASS' if passed else 'FAIL'),flush=True)
    return 0 if passed else 1


if __name__=='__main__':
    sys.exit(main())
