"""Real offline admission/child-process tests without optional CV or FFmpeg."""
from fractions import Fraction
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tools/tracking'))
import plate_contract as contract
import track_plate


def request():
    return dict(version=1, source_sha256='0'*64, stream_index=0,
                tracks=[dict(id='point',kind='point',position=[4.,5.])], corrections=[])


class PlateAdmissionTest(unittest.TestCase):
    def test_wrong_source_fails_before_optional_tools_and_logs_error_to_file_and_stderr(self):
        for optimized in (False,True):
            with self.subTest(optimized=optimized), tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                source=root/'plate.mp4'
                source.write_bytes(b'independently authored negative admission fixture')
                config=root/'request.json'
                config.write_text(json.dumps(request()),encoding='utf-8')
                output=root/'evidence'
                env=os.environ.copy()
                env.pop('MOTION_LOG',None)
                command=[sys.executable]+(['-O'] if optimized else [])+[str(ROOT/'tools/tracking/track_plate.py'),
                    '--source',str(source),'--request',str(config),'--output',str(output),
                    '--ffmpeg',str(root/'must-not-run'),'--ffprobe',str(root/'must-not-run')]
                result=subprocess.run(command,capture_output=True,text=True,env=env,timeout=30)
                self.assertEqual(result.returncode,1)
                self.assertEqual(result.stdout,'')
                self.assertIn('ERROR failed operation=track_plate phase=admission',result.stderr)
                self.assertIn('source SHA-256 differs',result.stderr)
                log=(output/'tracking.log').read_text(encoding='utf-8')
                self.assertIn('INFO start operation=track_plate',log)
                self.assertIn('ERROR failed operation=track_plate',log)
                report=json.loads((output/'report.json').read_text(encoding='utf-8'))
                self.assertEqual(report['status'],'failed')
                self.assertEqual(report['children'],[])
                self.assertFalse((output/'tracks.mstrack').exists())

    def test_duplicate_keys_and_nonfinite_coordinates_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'request.json'
            path.write_text('{"version":1,"version":1}',encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'duplicate JSON field'):
                contract.read_request(path)
            config=request()
            config['tracks'][0]['position'][0]=float('nan')
            path.write_text(json.dumps(config),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'invalid track coordinates'):
                contract.read_request(path)

    def test_exact_signed_irregular_pts_are_preserved_without_sorting_or_fps_inference(self):
        previous=None
        actual=[]
        for raw in (-3003, 0, 3004, 18007):
            previous=contract.timestamp(f'pts={raw}|width=8|height=6',[8,6],Fraction(2,90001),previous)
            actual.append(previous)
        self.assertEqual(actual,[-6006,0,6008,36014])
        for line in ('pts=N/A|width=8|height=6','pts=1|width=6|height=8','pts=-4000|width=8|height=6'):
            with self.subTest(line=line), self.assertRaises(ValueError):
                contract.timestamp(line,[8,6],Fraction(2,90001),previous)

    def test_threshold_suppresses_after_sixty_and_recovery_resets(self):
        counter=contract.ConsecutiveFailure()
        self.assertEqual([signal for _ in range(1000) if (signal:=counter.record(True)) is not None],[1,60])
        self.assertEqual(counter.count,60)
        self.assertIsNone(counter.record(False))
        self.assertEqual(counter.count,0)
        self.assertEqual([signal for _ in range(60) if (signal:=counter.record(True)) is not None],[1,60])

    def test_selected_stream_metadata_is_required_and_nominal_rate_is_separate(self):
        document=dict(streams=[dict(index=3,codec_type='video',width=8,height=6,time_base='2/90001',avg_frame_rate='30000/1001')])
        extent,base,rate=contract.metadata(document,3)
        self.assertEqual((extent,base,rate),([8,6],Fraction(2,90001),[30000,1001]))
        with self.assertRaisesRegex(ValueError,'not video'):
            contract.metadata(document,0)

    def test_child_is_stopped_on_failure_without_replacing_primary_cause(self):
        with tempfile.TemporaryDirectory() as directory:
            run=track_plate.Run(Path(directory)/'evidence')
            try:
                with self.assertRaisesRegex(ValueError,'primary admission failure'):
                    with run.child('controlled_failure',[sys.executable,'-c','import time;time.sleep(30)']):
                        raise ValueError('primary admission failure')
                self.assertIsInstance(run.report['children'][0]['exitCode'],int)
                self.assertIn('pid',run.report['children'][0])
            finally:
                run.close()

    def test_child_stderr_is_drained_but_retained_bytes_are_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            run=track_plate.Run(Path(directory)/'evidence')
            try:
                output=track_plate.capture(run,'bounded_stderr',[
                    sys.executable,'-c','import sys;sys.stderr.write("x"*400000)'],16)
                self.assertEqual(output,b'')
                row=run.report['children'][0]
                self.assertEqual(row['stderrBytes'],400000)
                self.assertEqual(row['stderrRetainedBytes'],256 << 10)
                self.assertEqual((run.output/'bounded_stderr.stderr.log').stat().st_size,256 << 10)
            finally:
                run.close()

    def test_correction_bounds_and_missing_observation_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'request.json'
            config=request()
            config['corrections']=[dict(track='point',source_index=5,visibility='occluded',observation=None)]
            path.write_text(json.dumps(config),encoding='utf-8')
            admitted=contract.read_request(path)
            with self.assertRaisesRegex(ValueError,'frame count'):
                contract.check_coordinates(admitted,[8,6],5)
            config['corrections'][0]['observation']={'kind':'point','point':[4,5]}
            path.write_text(json.dumps(config),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'must not contain'):
                contract.read_request(path)


if __name__=='__main__':
    unittest.main()
