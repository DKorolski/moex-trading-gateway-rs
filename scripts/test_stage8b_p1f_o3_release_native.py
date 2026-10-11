"""Offline checks of the native build input boundary; never opens SSH/Docker."""
import io
import tarfile
import unittest
from unittest.mock import Mock, patch

import stage8b_p1f_o3_release_native as n


class InputBoundary(unittest.TestCase):
    def archive(self, entries):
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode='w') as a:
            for name, kind in entries:
                m = tarfile.TarInfo(name)
                m.type = kind
                if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                    m.linkname = '/etc/passwd'
                a.addfile(m, io.BytesIO())
        return tarfile.open(fileobj=io.BytesIO(raw.getvalue()))

    def test_regular_public_files(self):
        with self.archive([('source', tarfile.DIRTYPE), ('source/Cargo.toml', tarfile.REGTYPE)]) as a:
            self.assertEqual(len(n.tar_members(a)), 2)

    def test_reject_links_special_and_escaping_paths(self):
        cases = [[('../outside', tarfile.REGTYPE)], [('/outside', tarfile.REGTYPE)],
                 [('a\\b', tarfile.REGTYPE)], [('link', tarfile.SYMTYPE)],
                 [('hardlink', tarfile.LNKTYPE)], [('fifo', tarfile.FIFOTYPE)],
                 [('same', tarfile.REGTYPE), ('same', tarfile.REGTYPE)]]
        for entries in cases:
            with self.subTest(entries=entries), self.archive(entries) as a:
                with self.assertRaises(ValueError):
                    n.tar_members(a)

    def test_task_syntax(self):
        compile(n.BUILD_TASK, '<native-build-task>', 'exec')

    def test_historical_baseline_is_not_rebound(self):
        self.assertEqual(n.r.REF, 'fb1ceb3dac40b365d0d23983af788ca1c9c72907')
        self.assertEqual(n.r.READMISSION_REF, '5df1c88299bbb7f1a70b709d3c01e2cf0a09d08c')

    def test_support_build_is_separate_from_five_release_slots(self):
        production, paths=n.build_plan()
        support, helpers=n.build_plan(True)
        self.assertEqual(set(paths),set(n.r.BINS))
        self.assertEqual(len(production),1)
        self.assertNotIn('--example',production[0]['command'])
        self.assertEqual(set(helpers),{'input-helper','bootstrap-source-witness'})
        self.assertFalse(set(paths)&set(helpers))
        self.assertEqual(len(support),2)
        self.assertIn('--bin',support[0]['command'])
        self.assertIn('--example',support[1]['command'])
        self.assertTrue(all('--offline' in step['command'] and '--locked' in step['command']
                            and '--no-default-features' in step['command'] for step in production+support))

    @patch.object(n.time, 'sleep')
    def test_transport_retry_is_bounded_and_read_only(self, sleep):
        run=Mock(side_effect=[n.TransportFailure('SSH timeout'), b'{"Running":true}'])
        self.assertEqual(n.read_container_state(run, 'own-container', 24), {'Running': True})
        self.assertEqual(run.call_count, 2)
        for call in run.call_args_list:
            self.assertEqual(call.args[1], ['docker', 'inspect', 'own-container',
                                           '--format', '{{json .State}}'])
            self.assertEqual(call.kwargs, {'timeout': 30})
        sleep.assert_called_once_with(5)
        run=Mock(side_effect=n.TransportFailure('SSH timeout'))
        with self.assertRaises(n.TransportFailure):
            n.read_container_state(run, 'own-container', 25)
        self.assertEqual(run.call_count, 3)

    @patch.object(n.time, 'sleep')
    def test_docker_failure_and_bad_json_are_not_retried(self, sleep):
        for failure in (RuntimeError('no container'), b'invalid-json'):
            run=Mock(side_effect=[failure])
            with self.assertRaises((RuntimeError, ValueError)):
                n.read_container_state(run, 'own-container', 0)
            self.assertEqual(run.call_count, 1)
        sleep.assert_not_called()


if __name__ == '__main__':
    unittest.main()
