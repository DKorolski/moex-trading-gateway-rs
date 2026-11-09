"""Read-only A/B witness. No activation, reset-failed, mkdir or cgroup writes.

Use as root on the host. An error is UNKNOWN/HOLD, never an empty sample.
This is one prerequisite, not complete auth/input/clock/stop qualification.
"""
import datetime
import errno
import json
import os
from pathlib import Path
import pwd
import stat
import subprocess

UNIT = 'signal-shadow.service'
ROOT = Path('/sys/fs/cgroup')
RELATIVE = Path('system.slice') / UNIT
FIELDS = ('Id', 'ActiveState', 'SubState', 'MainPID', 'ControlPID', 'ControlGroup',
          'Slice', 'InvocationID', 'ExecMainStartTimestampMonotonic', 'Job')

class Hold(Exception):
    pass

def command(*args):
    p = subprocess.run(args, capture_output=True, text=True, timeout=10)
    if p.returncode:
        raise Hold('READ_COMMAND_FAILED')
    return p.stdout

def unit_state():
    args = ['systemctl', 'show', UNIT]
    for key in FIELDS:
        args.extend(['-p', key])
    values = dict(line.split('=', 1) for line in command(*args).splitlines())
    if set(values) != set(FIELDS):
        raise Hold('UNIT_FIELDS_UNKNOWN')
    return values

def jobs():
    rows = []
    for line in command('systemctl', 'list-jobs', '--no-legend', '--no-pager', '--plain').splitlines():
        cols = line.split()
        if len(cols) >= 4 and cols[1] == UNIT:
            rows.append(dict(id=cols[0], unit=cols[1], type=cols[2], state=cols[3]))
        elif UNIT in cols:
            raise Hold('JOB_PARSE_UNKNOWN')
    return rows

def mount_witness():
    root = ROOT.lstat()
    if not stat.S_ISDIR(root.st_mode):
        raise Hold('CGROUP_ROOT_NOT_DIRECTORY')
    matches = []
    for line in Path('/proc/self/mountinfo').read_text().splitlines():
        before, after = line.split(' - ', 1)
        fields, fs = before.split(), after.split()
        if fields[4] == str(ROOT):
            if fs[0] != 'cgroup2':
                raise Hold('NOT_CGROUP2')
            matches.append(line)
    if len(matches) != 1:
        raise Hold('HOST_CGROUP_HIERARCHY_UNKNOWN')
    return dict(mount=matches[0], dev=root.st_dev, ino=root.st_ino,
                controllers=(ROOT / 'cgroup.controllers').read_text())

def related_process(dedicated_uid, owner, executable):
    return owner == dedicated_uid or Path(executable.removesuffix(' (deleted)')).name == 'finam-signal-shadow'

def process_witness():
    uid = pwd.getpwnam('moex-signal-shadow').pw_uid
    related = []
    scanned = 0
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        owner = path.stat().st_uid
        status = (path / 'stat').read_text()
        tail = status.rsplit(')', 1)[1].split()
        starttime = int(tail[19])
        try:
            executable = os.readlink(path / 'exe')
        except FileNotFoundError:
            # Kernel threads/zombies can have no executable; verify task identity
            # still exists. A vanished task is UNKNOWN for this bounded sample.
            if (path / 'stat').read_text() != status:
                raise Hold('PROC_CHANGED_WITHOUT_EXECUTABLE')
            executable = ''
        scanned += 1
        if related_process(uid, owner, executable):
            related.append(dict(pid=int(path.name), uid=owner, starttime_ticks=starttime,
                                executable=executable, cgroup=(path / 'cgroup').read_text()))
    return dict(dedicated_uid=uid, scanned=scanned, related=related,
                deleted_executable_suffix_checked=True)

def cgroup_sample(root):
    target = root / RELATIVE
    try:
        s = target.lstat()
    except FileNotFoundError as error:
        if error.errno != errno.ENOENT:
            raise
        # Prove the containing hierarchy is accessible; never hide EACCES/ENODEV.
        parent = target.parent.lstat()
        if not stat.S_ISDIR(parent.st_mode):
            raise Hold('EXPECTED_SLICE_UNKNOWN')
        with os.scandir(target.parent) as entries:
            names = [e.name for e in entries]
        if target.name in names:
            raise Hold('CGROUP_APPEARED_DURING_SAMPLE')
        return dict(kind='ABSENT', expected_path=str(target), lookup_errno=errno.ENOENT,
                    lookup_error='ENOENT', parent_accessible=True)
    if not stat.S_ISDIR(s.st_mode):
        raise Hold('SERVICE_CGROUP_NOT_DIRECTORY')
    samples = {}
    def walk(path):
        ids = [int(s) for s in (path / 'cgroup.procs').read_text().split()]
        events = dict(line.split() for line in (path / 'cgroup.events').read_text().splitlines())
        if events.get('populated') != '0' or ids:
            raise Hold('CGROUP_NOT_EMPTY')
        samples[str(path)] = dict(procs=ids, events=events)
        with os.scandir(path) as entries:
            for entry in entries:
                mode = entry.stat(follow_symlinks=False).st_mode
                if stat.S_ISLNK(mode):
                    raise Hold('UNEXPECTED_CGROUP_SYMLINK')
                if stat.S_ISDIR(mode):
                    walk(Path(entry.path))
        # A removed/changed node during traversal cannot be reclassified absent.
        if (path / 'cgroup.procs').read_text().split():
            raise Hold('CGROUP_CHANGED_DURING_SAMPLE')
    walk(target)
    return dict(kind='EMPTY', expected_path=str(target), samples=samples)

def sample(read_unit=unit_state, read_jobs=jobs, read_mount=mount_witness,
           read_processes=process_witness, root=ROOT):
    before = read_unit()
    if before['Id'] != UNIT or before['Slice'] != 'system.slice':
        raise Hold('UNIT_SLICE_MISMATCH')
    if (before['ActiveState'], before['SubState']) not in [('inactive', 'dead'), ('failed', 'failed')]:
        raise Hold('UNIT_NOT_TERMINAL')
    if before['MainPID'] != '0' or before['ControlPID'] != '0' or before['Job'] not in ('', '0'):
        raise Hold('PID_OR_UNIT_JOB_PRESENT')
    if before['ControlGroup'] not in ('', '/system.slice/' + UNIT):
        raise Hold('UNEXPECTED_CONTROL_GROUP')
    pending = read_jobs()
    if pending:
        raise Hold('UNIT_JOB_PRESENT')
    mount = read_mount()
    group = cgroup_sample(root)
    processes = read_processes()
    if processes['related']:
        raise Hold('RELATED_SHADOW_PROCESS_PRESENT')
    # A second complete sample catches a group disappearing/appearing between
    # process inspection and the terminal unit read. Do not combine mismatches.
    if cgroup_sample(root) != group or read_mount() != mount or read_jobs():
        raise Hold('CGROUP_MOUNT_OR_JOB_RACE')
    after = read_unit()
    if after != before:
        raise Hold('UNIT_STATE_RACE')
    return dict(status='VALID_ABSENT_NOT_RUNNING' if group['kind'] == 'ABSENT' else 'VALID_EMPTY_RECURSIVE_SAMPLE',
                captured_at=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                unit_before=before, unit_after=after, jobs=pending, mount=mount,
                cgroup=group, process_witness=processes,
                scope='point-in-time process/cgroup witness, not full operational readiness')

if __name__ == '__main__':
    try:
        result = sample()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, Hold) as error:
        # No raw commands, account details, environment or credentials are logged.
        result = dict(status='UNKNOWN_HOLD', error_class=type(error).__name__,
                      reason=str(error) if isinstance(error, Hold) else 'READ_OR_PARSE_FAILED',
                      errno=getattr(error, 'errno', None))
    print(json.dumps(result, indent=2))
    raise SystemExit(1 if result['status'] == 'UNKNOWN_HOLD' else 0)
