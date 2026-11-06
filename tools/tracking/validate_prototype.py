"""Measure actual optical tracking on authored video, with hidden analytic truth.

Truth is used only by this evaluator and explicit frame-30 manual corrections.
The tracker sees the encoded original, seeds and correction records only.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

from plate_contract import require
from track_plate import atomic_json, digest


def homography(index):
    return [[1.+.0007*index, .0003*index, .45*index],
            [-.0003*index, 1.+.0004*index, .18*index],
            [.000008*index, -.000005*index, 1.]]


def project(matrix, point):
    x,y=point
    bottom=matrix[2][0]*x+matrix[2][1]*y+matrix[2][2]
    return [(matrix[0][0]*x+matrix[0][1]*y+matrix[0][2])/bottom,
            (matrix[1][0]*x+matrix[1][1]*y+matrix[1][2])/bottom]


def encode(ffmpeg, output, images):
    command=[str(ffmpeg),'-v','error','-nostdin','-f','rawvideo','-pix_fmt','gray',
        '-video_size','256x192','-framerate','24','-i','pipe:0','-an',
        '-c:v','libvpx-vp9','-lossless','1','-deadline','good','-cpu-used','4',
        '-threads','1','-pix_fmt','yuv420p','-colorspace','bt709',
        '-color_primaries','bt709','-color_trc','bt709','-color_range','tv',str(output)]
    with output.with_suffix('.encode.stderr.log').open('wb') as error:
        child=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=error)
        try:
            for image in images:
                child.stdin.write(image.tobytes())
            child.stdin.close()
            require(child.wait()==0,'synthetic video encoding failed')
        finally:
            if child.poll() is None:
                child.terminate()
                child.wait()


def evaluate(ffmpeg, ffprobe, output):
    output.mkdir(exist_ok=False,parents=True)
    report=dict(status='running',scope='Authored analytic point/planar video; native import/render and photographed motion remain separate gates',
                limits=dict(pointMaxPixels=.75,planeMaxPixels=1.0),sourceFiles={})
    try:
        import cv2
        import numpy as np
        cv2.setNumThreads(1)
        random=np.random.default_rng(8817)
        texture=cv2.GaussianBlur(random.integers(20,236,(192,256),dtype=np.uint8),(3,3),.6)
        source=output/'motion.webm'
        truth=[homography(i) for i in range(96)]
        images=[cv2.warpPerspective(texture,np.array(matrix),(256,192),flags=cv2.INTER_LINEAR,
                borderMode=cv2.BORDER_CONSTANT,borderValue=0) for matrix in truth]
        for index in range(20,25):
            images[index]=np.zeros((192,256),np.uint8)
        encode(ffmpeg,source,images)
        cv2.imwrite(str(output/'reference.png'),images[0])
        point=[80.,70.]
        request=dict(version=1,source_sha256=digest(source),stream_index=0,
            tracks=[dict(id='feature',kind='point',position=point),
                    dict(id='plane',kind='plane',roi=[32.,24.,128.,104.])],
            corrections=[dict(track='feature',source_index=22,visibility='occluded',observation=None),
                         dict(track='feature',source_index=30,visibility='visible',observation=dict(kind='point',pixels=project(truth[30],point))),
                         dict(track='plane',source_index=30,visibility='visible',observation=dict(kind='plane',matrix=truth[30]))])
        config=output/'request.json'
        atomic_json(config,request)
        atomic_json(output/'independent-truth.json',dict(matrices=truth,point=point,occludedFrames=list(range(20,25)),manualResume=30))
        def run(name, source, config):
            command=[sys.executable,str(Path(__file__).with_name('track_plate.py')),
                '--source',str(source),'--request',str(config),'--output',str(output/name),
                '--ffmpeg',str(ffmpeg),'--ffprobe',str(ffprobe)]
            env=os.environ.copy()
            env.pop('MOTION_LOG',None)
            with (output/(name+'.stdout.log')).open('wb') as stdout,(output/(name+'.stderr.log')).open('wb') as stderr:
                child=subprocess.run(command,stdout=stdout,stderr=stderr,env=env)
            require(child.returncode==0,'actual tracker CLI failed: '+name)
            require((output/(name+'.stdout.log')).read_bytes()==b'','prototype polluted stdout')
            return json.loads((output/name/'tracks.mstrack').read_text(encoding='utf-8'))
        asset=run('motion-tracking',source,config)
        require(len(asset['source_pts'])==96,'original frame count changed')
        require(all(a<b for a,b in zip(asset['source_pts'],asset['source_pts'][1:])),'original PTS order changed')
        measured=[]
        probes=[[40.,32.],[144.,32.],[144.,112.],[40.,112.],[92.,72.]]
        for track in asset['tracks']:
            errors=[]
            for sample in track['samples']:
                index=sample['source_index']
                if 20 <= index < 30:
                    expected='occluded' if track['id']=='feature' and index==22 else 'lost'
                    require(sample['visibility']==expected and sample['observation'] is None,'occlusion/loss was replaced by invented geometry')
                    continue
                require(sample['visibility']=='visible','track unexpectedly missing outside declared occlusion')
                require(sample['manual_correction']==(index==30),'manual correction identity differs')
                if index in (0,30):
                    continue
                observation=sample['observation']
                if track['kind']=='point':
                    error=float(np.linalg.norm(np.array(observation['pixels'])-project(truth[index],point)))
                    require(error <= report['limits']['pointMaxPixels'],'point error exceeds unchanged budget')
                else:
                    error=max(float(np.linalg.norm(np.array(project(observation['matrix'],p))-project(truth[index],p))) for p in probes)
                    require(error <= report['limits']['planeMaxPixels'],'plane error exceeds unchanged budget')
                errors.append(error)
            require(len(errors)==84,'automatic measured frame coverage differs')
            measured.append(dict(track=track['id'],automaticFrames=len(errors),maxPixels=max(errors),rmsPixels=float(np.sqrt(np.mean(np.square(errors))))))
        report['measurements']=measured
        blank=output/'blank.webm'
        encode(ffmpeg,blank,[np.zeros((192,256),np.uint8) for _ in range(80)])
        blank_request=dict(version=1,source_sha256=digest(blank),stream_index=0,
            tracks=[dict(id='flat',kind='point',position=[80.,70.])],corrections=[])
        atomic_json(output/'blank-request.json',blank_request)
        missing=run('blank-tracking',blank,output/'blank-request.json')
        require(all(sample['visibility']=='lost' for sample in missing['tracks'][0]['samples'][1:]),'flat clip invented a visible track')
        for name in ('tracking.log','../blank-tracking.stderr.log'):
            text=(output/'blank-tracking'/name).read_text(encoding='utf-8')
            require(text.count('tracking degradation ')==1 and 'occurrences=60' in text,'sustained loss escalation/suppression differs')
        for file in (source,blank,config,output/'independent-truth.json'):
            report['sourceFiles'][file.name]=dict(bytes=file.stat().st_size,sha256=digest(file))
        report['status']='accepted_synthetic_external_prototype_native_and_real_motion_pending'
    except BaseException as cause:
        report.update(status='failed',failure=repr(cause))
        raise
    finally:
        atomic_json(output/'evaluation.json',report)
    return report


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('ffmpeg','ffprobe','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args()
    evaluate(args.ffmpeg,args.ffprobe,args.output)
