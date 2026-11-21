#!/usr/bin/env python3
"""Fixed accepted-ELF replacement after FAILED/1/23. No start/reload/signing.

Reuse existing custody, lock, fsync and atomic-replacement primitives. The
FAILED23 generated inputs remain in place: this installation does not consume
or retire them. Fresh-phase preparation must explicitly retire them before O2.
"""
import argparse
import copy
import json
import os
import platform
from pathlib import Path
import stat
import sys

import stage8b_p1f_o2_terminal_install as previous
c = previous.custody
require, sha, canonical = c.require, c.sha, c.canonical
parse = previous.strict_json
ROOT = Path(__file__).resolve().parents[1]
OLD = '782b651cb5e2cca5a2d4c8a6c611656df67492fc4e113378b056fe252bbe5a9b'
HEAD = '1864813e6375c89b92cb174e01a9fc52e58fc9ce133f117280d46820e7e96bbe'
SOURCE = '09dfecbe931267c5276b8b805d62d5a5a141ad95'
ARTIFACT = '9a6fafec0e6dc70f73bbfd973c60303fc37e38c20a2219bb8cc136a07f04aaf9'
BEFORE = '75f7d71b96d2f4aeb3395a3dd089fc6965dce71c8ee3e6b191191fcac37d2247'
ELFS = {
    '/usr/local/libexec/moex/stage8b-p1f-o2-materializer': ('45bcf423b5cfd8cb71fbc938c1c865de472b26c5c44787e3e2b9c819fb655a6f', 9670872),
    '/usr/local/libexec/moex/stage8b-p1f-o2-operator': ('592c6f321101648376c41bcb1043e33aaa4a7e19ad02ea2ec3a549fb24225e1b', 4198880),
    '/usr/local/libexec/moex/stage8b-p1-paper-supervisor': ('8cf872fb4fa395c5b68c95e29b5620dda56f964b82e77d568f5e24177bf6a105', 13495040),
}
CHANGES = (*ELFS, c.old.MANIFEST, c.MANIFEST)
TX = c.BASE + '/o2-replay-09dfecb-after-failed-23'
REVISION = 'replay-09dfecb-after-failed-23-v1'
CONFIG = '/etc/moex-finam-p1-paper'
STATE = '/var/lib/moex-finam-p1-paper/state'
REPEAT = '/root/moex-o2-boot-id-repeat-20261008'
RETAINED = tuple(p for p, _ in previous.RETAINED_DIRS) + (
    previous.TRANSACTION,
    c.BASE + '/o2-boot-id-after-failed-15-20261008',
    c.BASE + '/o2-bootstrap-boot-id-after-failed-15-20261008',
    '/root/moex-o2-bounded-20261008', REPEAT,
    c.BASE + '/o2-credential-817985c-after-failed-19',
    c.BASE + '/o2-retire-failed-19-20261008',
    '/root/moex-o2-credential-once-20261008',
)
TREES = (CONFIG, c.CONTROL, STATE, c.STAGING, *RETAINED)
ABSENT = ('/root/moex-o2-credential-once-20261008/auth.private.json',
          REPEAT + '/auth.private.json', '/root/moex-o2-bounded-20261008/auth.private.json',
          '/run/moex-finam-p1f-o2-input',
          '/run/credentials/moex-finam-p1f-o2-materializer.service',
          '/run/credentials/moex-finam-p1-paper-bootstrap.service')
CONFIRM = 'INSTALL_09DFECB_PRESERVE_FAILED_23_WITHOUT_ACTIVATION'
ROLLBACK = 'ROLLBACK_09DFECB_PRESERVE_FAILED_23'


def terminal(head):
    require(head['state'] == 'FAILED' and type(head['authority_generation']) is int
            and head['authority_generation'] == 1 and type(head['latest_sequence']) is int
            and head['latest_sequence'] == 23 and head['latest_event_sha256'] == HEAD
            and head['active_manifest_sha256'] is None and head['deadline_utc'] is None,
            'exact FAILED/1/23 required')


def inventory(directory):
    """Never read/hash credential/key bytes; public state stays hash-only."""
    require(directory.is_dir() and not directory.is_symlink(), 'missing invariant directory')
    out = {}
    for p in (directory, *sorted(directory.rglob('*'))):
        s = p.lstat()
        require(len(out) < 4000 and (stat.S_ISDIR(s.st_mode) or stat.S_ISREG(s.st_mode)), 'unsafe inventory node')
        value = dict(uid=s.st_uid, gid=s.st_gid, mode=oct(stat.S_IMODE(s.st_mode)), type=stat.S_IFMT(s.st_mode))
        if stat.S_ISREG(s.st_mode):
            require(s.st_nlink == 1, 'inventory hardlink')
            value.update(size=s.st_size, nlink=s.st_nlink)
            secret = 'credentials' in p.parts or p.name == 'retired-2' or p.suffix in ('.key', '.ed25519') or 'auth.private' in p.name
            if secret:
                value.update(content_not_read=True, inode=s.st_ino, device=s.st_dev, mtime_ns=s.st_mtime_ns, ctime_ns=s.st_ctime_ns)
            else:
                value['sha256'] = sha(c.file_bytes(Path('/'), str(p), stat.S_IMODE(s.st_mode), s.st_gid))
        out[str(p.relative_to(directory))] = value
    return out


def properties(unit):
    keys = ['Id', 'LoadState', 'ActiveState', 'SubState', 'MainPID', 'ControlPID', 'ControlGroup',
            'FragmentPath', 'DropInPaths', 'NeedDaemonReload', 'Job', 'Restart', 'NRestarts',
            'ExecStart', 'User', 'Group', 'ProcSubset', 'PrivateNetwork', 'ProtectSystem',
            'NoNewPrivileges', 'CapabilityBoundingSet', 'RestrictAddressFamilies',
            'NextElapseUSecRealtime', 'NextElapseUSecMonotonic', 'Persistent']
    text = c.command(['systemctl', 'show', unit, '--no-pager', '--property=' + ','.join(keys)])
    rows = [line.split('=', 1) for line in text.splitlines()]
    require(len(dict(rows)) == len(rows), 'duplicate properties')
    return dict(rows)


def host_observation():
    """Read-only systemd/proc checks. No Redis or FINAM request."""
    require(platform.system() == 'Linux' and platform.machine() == 'x86_64', 'native Linux amd64 required')
    require(tuple(int(n) for n in os.confstr('CS_GNU_LIBC_VERSION').split()[1].split('.')) >= (2, 34), 'GLIBC version')
    require(c.command(['ssh-keygen', '-lf', '/etc/ssh/ssh_host_ed25519_key.pub', '-E', 'sha256']).split()[1] == c.HOST_KEY, 'target host key')
    p1 = sorted(p.name for p in Path('/etc/systemd/system').glob('moex-finam-p1*') if p.suffix in ('.service', '.timer'))
    units = {n: properties(n) for n in (*c.P0_UNITS, *p1) if '@' not in n}
    for n in c.P0_UNITS:
        require(units[n]['ActiveState'] == 'active' and units[n]['SubState'] == 'running'
                and int(units[n]['MainPID']) > 0, 'P0 is not running')
    for n in p1:
        if '@' in n:
            continue  # Uninstantiated template: file pinned, no systemctl show.
        v = units[n]
        require(v['DropInPaths'] == '' and v['NeedDaemonReload'] == 'no'
                and v['Job'] in ('', '0'), 'P1 unit overrides/reload/job')
        if n.endswith('.timer'):
            require(v['ActiveState'] in ('active', 'inactive') and v['SubState'] in ('elapsed', 'dead')
                    and v['NextElapseUSecRealtime'] == ''
                    and v['NextElapseUSecMonotonic'] in ('', 'infinity')
                    and v['Persistent'] == 'no', 'timer can fire')
        elif '@' not in n:
            require(v['MainPID'] == v['ControlPID'] == '0' and v['ControlGroup'] == ''
                    and v['ActiveState'] in ('inactive', 'failed'), 'P1 not stopped: ' + n)
            if n != 'moex-finam-p1-paper.service':
                require(v['Restart'] == 'no', 'bounded service restart: ' + n)
    listed = {}
    for action in ('list-units', 'list-unit-files'):
        raw = c.command(['systemctl', action, '--all', '--plain', '--no-legend', '--no-pager', 'moex-finam-p1*'])
        names = sorted(line.split()[0] for line in raw.splitlines() if line.split())
        require(set(names) <= set(p1), 'unknown P1 unit')
        listed[action] = names
    require(not c.command(['systemctl', 'list-jobs', '--no-legend', '--no-pager']).strip(), 'systemd jobs')
    c.assert_no_p1_processes(Path('/proc'), 987)
    p0_executables = {}
    for name in c.P0_UNITS:
        proc = Path('/proc') / units[name]['MainPID']
        executable = proc / 'exe'
        # /proc executable bytes are public program code, never environ or argv
        # credentials. Bind process start time too, so PID reuse is not equality.
        raw = executable.read_bytes()
        p0_executables[name] = dict(path=os.readlink(executable), size=len(raw), sha256=sha(raw),
            start_ticks=(proc/'stat').read_text().rsplit(')', 1)[1].split()[19])
    return dict(units=units, listed=listed,
                unit_sha256={n: sha(c.file_bytes(Path('/'), '/etc/systemd/system/' + n, 0o644, 0)) for n in (*c.P0_UNITS, *p1)},
                account=list(c.old.account_ids(Path('/'))), p0_executables=p0_executables,
                platform=platform.machine(), libc=os.confstr('CS_GNU_LIBC_VERSION'))


def observe(root):
    path = lambda p: c.old.rooted(root, p)
    head = parse(c.file_bytes(root, c.CONTROL + '/authority/history-head.json', 0o440, 987))
    terminal(head)
    require(all(not os.path.lexists(path(p)) for p in ABSENT), 'transient credential/input exists')
    require({str(p.relative_to(path(STATE))) for p in path(STATE).rglob('*')} <= {'.stage8b-p1-first-boot-quarantine'}, 'durable state initialized')
    return dict(history_head=head, trees={p: inventory(path(p)) for p in TREES}, host=host_observation())


def replacements(old_raw, compat_raw, binaries, installer_sha):
    require(sha(old_raw) == OLD, 'wrong installed predecessor')
    old = parse(old_raw)
    require(set(binaries) == set(ELFS), 'ELF slots')
    for p, (digest, size) in ELFS.items():
        require(sha(binaries[p]) == digest and len(binaries[p]) == size, 'accepted ELF bytes: ' + p)
    require(sha(compat_raw) == old['payload'][c.old.MANIFEST]['sha256'], 'compat predecessor')
    compat = parse(compat_raw)
    compat['binary_sha256'] = ELFS[c.old.BINARY_PATH][0]
    for p in compat['managed_payload_sha256']:
        if p in binaries:
            compat['managed_payload_sha256'][p] = sha(binaries[p])
    payload = dict(binaries)
    payload[c.old.MANIFEST] = canonical(compat)
    new = copy.deepcopy(old)
    for p, raw in payload.items():
        new['payload'][p].update(sha256=sha(raw), size=len(raw))
    new.update(artifact_ref=SOURCE, artifact_sha256=ARTIFACT, predecessor_manifest_sha256=OLD,
               installer_sha256=installer_sha, update_revision=REVISION)
    payload[c.MANIFEST] = canonical(new)
    return payload


def validate_prepared(directory):
    require(not directory.is_symlink() and not any(p.is_symlink() for p in directory.rglob('*')), 'prepared symlink')
    spec = parse((directory / 'spec.json').read_bytes())
    before = (directory / 'before.json').read_bytes()
    require(sha(before) == spec['before_sha256'] == BEFORE, 'snapshot binding')
    require(spec['installation_authorized'] is False and spec['execution_authorized'] is False
            and spec['expected_terminal'] == 'FAILED/1/23' and spec['compiled_source_ref'] == SOURCE
            and spec['accepted_artifact_sha256'] == ARTIFACT and spec['old_manifest_sha256'] == OLD, 'scope/lineage')
    expected = parse(before)
    terminal(expected['history_head'])
    old_raw = (directory / 'old/installation-o2-v1.json').read_bytes()
    compat = (directory / 'old/installation-v1.json').read_bytes()
    own = sha(Path(__file__).read_bytes())
    require(own == spec['installer_sha256'], 'installer binding')
    payload = replacements(old_raw, compat, {p: (directory / 'payload' / Path(p).name).read_bytes() for p in ELFS}, own)
    old = parse(old_raw)
    require(set(spec['updates']) == set(CHANGES)
            and {p.name for p in (directory / 'payload').iterdir()} == {Path(p).name for p in CHANGES}, 'write inventory')
    for p in CHANGES:
        raw = (directory / 'payload' / Path(p).name).read_bytes()
        require(raw == payload[p] and spec['updates'][p] == dict(
            old_sha256=OLD if p == c.MANIFEST else old['payload'][p]['sha256'], new_sha256=sha(raw), size=len(raw)), 'exact delta: ' + p)
    require(sha(payload[c.MANIFEST]) == spec['new_manifest_sha256'], 'new binding')
    public = directory / 'old-tree'
    require({p.relative_to(public).as_posix() for p in public.rglob('*') if p.is_file()} ==
            {p.lstrip('/') for p in old['payload'] if p not in ELFS} | {c.MANIFEST.lstrip('/')}, 'old public inventory')
    for p, e in old['payload'].items():
        if p not in ELFS:
            raw = (public / p.lstrip('/')).read_bytes()
            require(sha(raw) == e['sha256'] and len(raw) == e['size'], 'old public bytes')
    require((public / c.MANIFEST.lstrip('/')).read_bytes() == old_raw, 'old public manifest')
    coverage = parse((public / (c.O2DIR + '/source-template.json').lstrip('/')).read_bytes())['history_coverage']
    policy = parse((public / (c.O2DIR + '/materialization-policy.json').lstrip('/')).read_bytes())
    cal = spec['calendar']
    require(cal['unchanged'] is True and cal['session_date'] == '2026-10-08'
            and cal['history_coverage'] == coverage
            and cal['prior_dates'] == ['2026-10-02', '2026-10-05', '2026-10-06', '2026-10-07']
            and cal['bars_start_utc'] == policy['bars_start_utc'] == '2026-10-02T04:00:00Z'
            and cal['bars_end_utc'] == cal['execution_window_end_utc'] == cal['expires_at_utc'] == policy['bars_end_utc'] == '2026-10-08T20:50:00Z'
            and cal['execution_window_start_utc'] == '2026-10-08T04:10:00Z', 'calendar binding')
    return spec, expected, old, payload


class Update(previous.Update):
    """Existing five-slot fsync/backup protocol, fixed successor, no new authority."""
    def __init__(self, root, directory):
        self.root = c.old.validate_root(root)
        self.directory = directory
        self.spec, self.expected, self.old, self.payload = validate_prepared(directory)
        self.uid, self.gid = c.old.account_ids(root)
        require((self.uid, self.gid) == (987, 987), 'service identity drift')

    def invariants(self):
        require(observe(self.root) == self.expected, 'terminal/P0/custody/stopped state drift')

    def verify_files(self, state):
        for p, entry in self.old['payload'].items():
            raw = self.read(p)
            require(len(raw) == entry['size'] or p in CHANGES, 'old size drift')
            options = {entry['sha256']}
            if p in CHANGES:
                new = self.spec['updates'][p]['new_sha256']
                options = {'old': options, 'new': {new}, 'mixed': options | {new}}[state]
            require(sha(raw) in options, 'foreign installed bytes: ' + p)
        options = {'old': {OLD}, 'new': {self.spec['new_manifest_sha256']}, 'mixed': {OLD, self.spec['new_manifest_sha256']}}[state]
        require(sha(self.read(c.MANIFEST)) in options, 'manifest drift')

    def write_journal(self, state):
        c.atomic_write(self.root, TX + '/journal.json', canonical(dict(domain=REVISION,
            old_sha256=OLD, new_sha256=self.spec['new_manifest_sha256'], state=state)), 0o600, 0)

    def journal(self):
        p = self.path(TX)
        s = p.lstat()
        require(stat.S_ISDIR(s.st_mode) and s.st_uid == s.st_gid == 0 and stat.S_IMODE(s.st_mode) == 0o700, 'transaction custody')
        require({q.name for q in p.iterdir()} == {'journal.json', *[f'before-{i}' for i in range(len(CHANGES))]}, 'transaction inventory')
        value = parse(c.file_bytes(self.root, TX + '/journal.json', 0o600, 0))
        require(set(value) == {'domain', 'old_sha256', 'new_sha256', 'state'} and value['domain'] == REVISION
                and value['old_sha256'] == OLD and value['new_sha256'] == self.spec['new_manifest_sha256']
                and value['state'] in ('PREPARED', 'APPLIED', 'ROLLING_BACK', 'ROLLED_BACK'), 'journal binding')
        for i, p in enumerate(CHANGES):
            require(sha(c.file_bytes(self.root, TX + f'/before-{i}', 0o600, 0)) == self.spec['updates'][p]['old_sha256'], 'backup drift')
        return value

    def transition(self, action):
        require(action in ('preflight', 'apply', 'resume', 'rollback'), 'unknown action')
        with self.locks():
            self.invariants()
            if not c.exists(self.root, TX):
                self.verify_files('old')
                if action == 'preflight':
                    return dict(state='EXACT_OLD_FAILED_23', installation_performed=False)
                require(action == 'apply', 'no prior transaction')
                c.old.verify_secure_directory_chain(self.root, self.path(TX).parent)
                self.path(TX).mkdir(mode=0o700)
                c.sync_dir(self.path(TX).parent)
                for i, p in enumerate(CHANGES):
                    c.atomic_write(self.root, TX + f'/before-{i}', self.read(p), 0o600, 0)
                self.write_journal('PREPARED')
            j = self.journal()
            self.verify_files({'APPLIED': 'new', 'ROLLED_BACK': 'old'}.get(j['state'], 'mixed'))
            if action == 'preflight':
                return dict(state=j['state'], installation_performed=False)
            if action in ('apply', 'resume'):
                require(j['state'] in ('PREPARED', 'APPLIED'), 'cannot replay rolled-back transaction')
                desired, result = 'new', 'APPLIED'
            else:
                desired, result = 'old', 'ROLLED_BACK'
                self.write_journal('ROLLING_BACK')
            for i, p in enumerate(CHANGES):
                self.invariants()
                self.verify_files('mixed')
                raw = self.payload[p] if desired == 'new' else c.file_bytes(self.root, TX + f'/before-{i}', 0o600, 0)
                if raw != self.read(p):
                    entry = self.old['payload'].get(p, dict(mode='0644', group='root'))
                    c.atomic_write(self.root, p, raw, int(entry['mode'], 8), self.gid if entry['group'] == 'service' else 0)
            self.verify_files(desired)
            self.invariants()
            self.write_journal(result)
            return dict(state=result, history_preserved=True, generated_inputs_preserved=True,
                        activation_performed=False, daemon_reload_performed=False,
                        redis_contact=False, finam_contact=False)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=('preflight', 'apply', 'resume', 'rollback'))
    p.add_argument('--input', type=Path, default=ROOT / 'installation-replay')
    p.add_argument('--confirm')
    a = p.parse_args()
    require(a.confirm == (None if a.action == 'preflight' else ROLLBACK if a.action == 'rollback' else CONFIRM), 'explicit separate authorization required')
    print(json.dumps(Update(Path('/'), a.input).transition(a.action), sort_keys=True))


if __name__ == '__main__':
    try:
        main()
    except (c.Error, OSError, ValueError, KeyError) as error:
        print('replay-install: FAIL ' + str(error), file=sys.stderr)
        raise SystemExit(1)
