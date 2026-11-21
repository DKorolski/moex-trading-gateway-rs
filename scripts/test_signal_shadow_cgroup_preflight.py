import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import signal_shadow_cgroup_preflight as c

class Cgroup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / 'system.slice').mkdir()
        self.state = dict(Id=c.UNIT, Slice='system.slice', ActiveState='failed', SubState='failed',
                          MainPID='0', ControlPID='0', ControlGroup='', Job='0', InvocationID='retained',
                          ExecMainStartTimestampMonotonic='1234')
        self.processes = dict(related=[], dedicated_uid=996, scanned=10)
    def sample(self, **kw):
        args=dict(read_unit=lambda:copy.deepcopy(self.state), read_jobs=lambda:[],
                  read_mount=lambda:dict(filesystem='cgroup2'), read_processes=lambda:self.processes, root=self.root)
        args.update(kw)
        return c.sample(**args)
    def group(self):
        path=self.root/c.RELATIVE;path.mkdir()
        (path/'cgroup.procs').write_text('');(path/'cgroup.events').write_text('populated 0\nfrozen 0\n')
        return path
    def test_absent_is_distinct_not_empty_samples(self):
        r=self.sample();self.assertEqual(r['status'],'VALID_ABSENT_NOT_RUNNING')
        self.assertEqual(r['cgroup']['lookup_errno'],2);self.assertNotIn('samples',r['cgroup'])
    def test_empty_recursive(self):
        path=self.group();child=path/'child';child.mkdir()
        (child/'cgroup.procs').write_text('');(child/'cgroup.events').write_text('populated 0\n')
        r=self.sample();self.assertEqual(r['status'],'VALID_EMPTY_RECURSIVE_SAMPLE')
        self.assertEqual(len(r['cgroup']['samples']),2)
    def test_pid_job_and_related_deleted_exe_block(self):
        self.assertTrue(c.related_process(996,0,'/opt/release/finam-signal-shadow (deleted)'))
        self.assertTrue(c.related_process(996,996,'/usr/bin/sleep'))
        self.assertFalse(c.related_process(996,0,'/usr/bin/other'))
        for key in ('MainPID','ControlPID','Job'):
            with self.subTest(key=key):
                self.state[key]='10'
                with self.assertRaises(c.Hold):self.sample()
                self.state[key]='0'
        with self.assertRaises(c.Hold):self.sample(read_jobs=lambda:[dict(type='start')])
        self.processes['related']=[dict(pid=10,executable='/x/finam-signal-shadow (deleted)')]
        with self.assertRaises(c.Hold):self.sample()
    def test_unit_race(self):
        calls=[]
        def state():
            result=copy.deepcopy(self.state);calls.append(1)
            if len(calls)>1:result['InvocationID']='changed'
            return result
        with self.assertRaises(c.Hold):self.sample(read_unit=state)
    def test_group_disappears_during_read_is_unknown(self):
        path=self.group();original=Path.read_text
        def read(p,*args,**kwargs):
            if p==path/'cgroup.procs':raise FileNotFoundError(2,'race')
            return original(p,*args,**kwargs)
        with patch.object(Path,'read_text',read),self.assertRaises(FileNotFoundError):self.sample()
    def test_permission_error_never_becomes_absent(self):
        original=Path.lstat
        def check(p,*args,**kwargs):
            if p==self.root/c.RELATIVE:raise PermissionError(13,'denied')
            return original(p,*args,**kwargs)
        with patch.object(Path,'lstat',check),self.assertRaises(PermissionError):self.sample()
    def test_population_and_wrong_slice_block(self):
        p=self.group();(p/'cgroup.events').write_text('populated 1\n')
        with self.assertRaises(c.Hold):self.sample()
        self.state['Slice']='other.slice'
        with self.assertRaises(c.Hold):self.sample()

if __name__=='__main__':unittest.main()
