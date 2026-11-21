#!/usr/bin/env python3
"""Exact stopped config-only correction, never activation or O2 history repair.

The 0e89208 installed ELF/template/manifests and accepted O2 root are retained.
Reuse existing custody/locks/atomic-write primitives. Only supervisor.json
and this correction's separate backup/journal may be written, after permission.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import stat
import sys

import stage8b_p1f_o3_install_inventory as inv

c = inv.c
require, sha, parse = c.require, c.sha, inv.prior.parse


def canonical(value):
    # Runtime config/Redis use compact serde JSON without a final LF, unlike
    # the existing installer helper's newline-terminated journal encoding.
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False).encode()
ROOT = Path(__file__).resolve().parents[1]
CONFIG = inv.prior.CONFIG + '/supervisor.json'
TX = c.BASE + '/o3-config-binding-after-completed-26-v1'
INSTALL_TX = c.BASE + '/o3-fb1ceb3-after-completed-26'
BASELINE_SHA = 'e786362486e135081e2358d867fc6fa56db441e66ba0e35a2d4f44585e18a3f5'
OLD_CONFIG = '2d28d3c2d9fb3e4933a2f89e3b751dfe67d56b14d1e4f542bbc838e0513c56d6'
NEW_CONFIG = '3a031f24ac91990bb4b00308fad93d48b36af09d135ab5d4ecc62e4702e5c3e8'
OLD_PIN = '3c9ddebe1395e5fdcf306be67988698fd2f1a20e1ae8c19e376e16027956ebbc'
NEW_PIN = '4b9162f0f87d55f4cca3b8efac9d33491f0d3f421eb090e8cdd78fcb0f3e86b3'
IDENTITY = 'fbd9ceb7ee964944e79db559c4e4d5cc7af54578b653ce218e183d01fa1444c3'
CONFIRM = 'CORRECT_O3_CONFIG_PIN_PRESERVE_COMPLETED_26_NO_ACTIVATION'
FIXTURE = ROOT/'scripts/fixtures/o3-accepted-o2-supervisor.json'


def corrected(old):
    require(sha(old) == OLD_CONFIG, 'not exact accepted O2 config')
    value = parse(old)
    require(canonical(value) == old and value['redis_deployment_manifest_sha256'] == OLD_PIN,
            'old config canonical pin')
    value['redis_deployment_manifest_sha256'] = NEW_PIN
    new = canonical(value)
    require(sha(new) == NEW_CONFIG, 'unexpected corrected bytes')
    return new


def independent_manifest(config):
    # Offline cross-check; Rust production constructor remains the authority.
    value = parse((ROOT/'docs/stage-8/stage8b-p1e-redis-deployment-manifest-v1.json').read_bytes())
    for k in ('runtime_config_fingerprint_sha256', 'instrument_map_fingerprint_sha256', 'deployment_generation'):
        value[k] = config['bootstrap'][k]
    value['consumer_generation'] = config['bootstrap']['command_consumer_generation']
    value['operational_identity_sha256'] = IDENTITY
    return canonical(value)


def expected_files(baseline):
    require(sha(baseline) == BASELINE_SHA, 'accepted installation postflight binding')
    snapshot = parse(baseline)
    old = FIXTURE.read_bytes().rstrip(b'\n')
    new = corrected(old)
    manifest = independent_manifest(parse(new))
    require(sha(manifest) == NEW_PIN, 'manifest shape drift')
    require(snapshot['observed']['trees'][inv.prior.CONFIG]['supervisor.json']['sha256'] == OLD_CONFIG,
            'accepted config inventory binding')
    return {'baseline.json': baseline, 'supervisor.before.json': old,
            'supervisor.after.json': new, 'redis-manifest.json': manifest}


def load(directory):
    require(not directory.is_symlink() and directory.is_dir(), 'package directory')
    names = {'baseline.json', 'supervisor.before.json', 'supervisor.after.json', 'redis-manifest.json'}
    require({p.name for p in directory.iterdir()} == names, 'package inventory')
    data = {}
    for n in names:
        p = directory/n; s = p.lstat()
        require(stat.S_ISREG(s.st_mode) and s.st_nlink == 1 and s.st_size < 1024*1024, 'package file custody')
        data[n] = p.read_bytes()
    require(data == expected_files(data['baseline.json']), 'exact config-only package')
    return data


class Update:
    # Same existing lock inodes; no new authority mechanism or lock creation.
    locks = inv.prior.previous.Update.locks

    def __init__(self, root, data):
        self.root = c.old.validate_root(root)
        self.data = data
        require(data == expected_files(data['baseline.json']), 'package changed')
        self.snapshot = parse(data['baseline.json'])
        require(c.old.account_ids(root) == (987, 987), 'service identity drift')

    def path(self, n): return c.old.rooted(self.root, n)

    def read_config(self): return c.file_bytes(self.root, CONFIG, 0o440, 987)

    def invariants(self):
        actual = inv.observe(self.root)
        expected = copy.deepcopy(self.snapshot['observed'])
        raw = self.read_config()
        require(raw in (self.data['supervisor.before.json'], self.data['supervisor.after.json']),
                'foreign current config')
        entry = expected['trees'][inv.prior.CONFIG]['supervisor.json']
        entry.update(sha256=sha(raw), size=len(raw))
        require(actual == expected, 'root/receipt/history/key custody/P0 drift')
        for n, e in self.snapshot['installed'].items():
            raw = c.file_bytes(self.root, n, int(e['mode'], 8), e['gid'])
            require(sha(raw) == e['sha256'] and len(raw) == e['size'], 'installed payload drift: '+n)
        require(inv.inventory(self.path(INSTALL_TX)) == self.snapshot['transaction_inventory'],
                'accepted installation transaction drift')
        require(c.command(['redis-cli', '-h', '127.0.0.1', '-p', '6379', '-n', '15', 'DBSIZE']).strip() == '0',
                'DB15 not empty; no repair/flush')

    def receipt(self, state):
        return canonical(dict(domain='moex.o3.config-binding-correction.v1', state=state,
            accepted_installation_postflight_sha256=BASELINE_SHA,
            old_config_sha256=OLD_CONFIG, new_config_sha256=NEW_CONFIG,
            redis_manifest_sha256=NEW_PIN, predecessor_event_sha256=inv.HEAD,
            activation_performed=False, historical_receipts_modified=False))

    def journal(self):
        p = self.path(TX); s = p.lstat()
        require(stat.S_ISDIR(s.st_mode) and s.st_uid == s.st_gid == 0
                and stat.S_IMODE(s.st_mode) == 0o700, 'correction directory custody')
        names = {q.name for q in p.iterdir()}
        require(names <= {'supervisor.before.json', 'journal.json'}, 'foreign correction files')
        if 'supervisor.before.json' in names:
            require(c.file_bytes(self.root, TX+'/supervisor.before.json', 0o600, 0)
                    == self.data['supervisor.before.json'], 'backup drift')
        if 'journal.json' not in names:
            require(self.read_config() == self.data['supervisor.before.json'], 'new config without prepared journal')
            return None
        require('supervisor.before.json' in names, 'journal without backup')
        raw = c.file_bytes(self.root, TX+'/journal.json', 0o600, 0)
        state = parse(raw)['state']
        require(state in ('PREPARED', 'APPLIED') and raw == self.receipt(state), 'journal drift')
        if state == 'APPLIED':
            require(self.read_config() == self.data['supervisor.after.json'], 'applied config drift')
        return state

    def transition(self, action):
        require(action in ('preflight', 'apply', 'resume'), 'unknown action')
        with self.locks():
            self.invariants()
            present = c.exists(self.root, TX)
            if not present:
                require(self.read_config() == self.data['supervisor.before.json'], 'unrecorded config change')
                require(action != 'resume', 'no prior correction')
                if action == 'preflight':
                    return dict(state='EXACT_OLD_CONFIG', activation_performed=False)
                c.old.verify_secure_directory_chain(self.root, self.path(TX).parent)
                self.path(TX).mkdir(mode=0o700); c.sync_dir(self.path(TX).parent)
            state = self.journal()
            if action == 'preflight':
                return dict(state=state or 'INCOMPLETE_PREPARATION', activation_performed=False)
            # Recovery is explicit, pinned to this same transaction and inputs.
            # No retry/second phase, no historical receipt/authority mutation.
            if state is None:
                backup = TX+'/supervisor.before.json'
                if not c.exists(self.root, backup):
                    c.atomic_write(self.root, backup, self.data['supervisor.before.json'], 0o600, 0)
                c.atomic_write(self.root, TX+'/journal.json', self.receipt('PREPARED'), 0o600, 0)
            self.invariants()
            if self.read_config() != self.data['supervisor.after.json']:
                c.atomic_write(self.root, CONFIG, self.data['supervisor.after.json'], 0o440, 987)
            self.invariants()
            if state != 'APPLIED':
                c.atomic_write(self.root, TX+'/journal.json', self.receipt('APPLIED'), 0o600, 0)
            require(self.journal() == 'APPLIED', 'postflight journal')
            return dict(state='APPLIED', config_sha256=NEW_CONFIG, activation_performed=False,
                        historical_receipts_modified=False, redis_writes=False)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('action', choices=('prepare', 'verify', 'preflight', 'apply', 'resume'))
    ap.add_argument('directory', type=Path)
    ap.add_argument('--baseline', type=Path)
    ap.add_argument('--confirm')
    a = ap.parse_args()
    if a.action == 'prepare':
        require(a.baseline is not None and a.confirm is None, 'prepare input')
        data = expected_files(a.baseline.read_bytes())
        a.directory.mkdir(parents=True, exist_ok=False)
        for n, b in data.items():
            with (a.directory/n).open('xb') as f: f.write(b)
    data = load(a.directory)
    if a.action in ('prepare', 'verify'):
        print(json.dumps(dict(status='PASS_OFFLINE', config_delta=['redis_deployment_manifest_sha256'],
                              old_sha256=OLD_CONFIG, new_sha256=NEW_CONFIG, activation_performed=False)))
        return
    require(os.geteuid() == 0, 'root required')
    require(a.confirm == (None if a.action == 'preflight' else CONFIRM), 'exact confirmation required')
    print(json.dumps(Update(Path('/'), data).transition(a.action), sort_keys=True))


if __name__ == '__main__':
    try: main()
    except (c.Error, OSError, ValueError, KeyError) as e:
        print('o3-config-binding: FAIL '+str(e), file=sys.stderr)
        raise SystemExit(1)
