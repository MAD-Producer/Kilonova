"""Reproduce #88's external real-photo calibration, with disjoint validation.

No product dependency or bundled photograph. Use the separately installed
opencv-python-headless 5.0.0.93 and NumPy 2.4.6 credited in ATTRIBUTION.md.
"""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import urllib.request

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / 'engineering/validation/data/tracking88-photographs-sources.json'
HELD_OUT = ['left03.jpg', 'left08.jpg', 'left14.jpg']


def calibration(source, output, download=False):
    output.mkdir(exist_ok=False, parents=True)
    manifest = json.loads(MANIFEST.read_text(encoding='utf-8'))
    report = dict(status='running', sourceManifest=manifest,
                  implementation=dict(opencv=importlib.metadata.version('opencv-python-headless'),
                                      numpy=np.__version__),
                  scope='Real separate board poses; not continuous motion or a full camera solver',
                  scaleConvention='One square assigned one project metre as a scale gauge; physical square size unknown',
                  limits=dict(heldOutRmsPixels=1., heldOutMaxPixels=2.5), views=[])
    try:
        assert report['implementation'] == dict(opencv='5.0.0.93', numpy='2.4.6')
        source.mkdir(exist_ok=True, parents=True)
        for item in manifest['files']:
            path = source / item['name']
            if download and not path.exists():
                prefix = f"https://raw.githubusercontent.com/opencv/opencv/{manifest['revision']}/"
                assert item['source'].startswith(prefix)
                with urllib.request.urlopen(item['source'], timeout=30) as response:
                    content = response.read(2_000_001)
                assert len(content) == item['bytes']
                assert hashlib.sha256(content).hexdigest() == item['sha256']
                path.write_bytes(content)
            content = path.read_bytes()
            assert len(content) == item['bytes'] and hashlib.sha256(content).hexdigest() == item['sha256'], item['name']
        names = [i['name'] for i in manifest['files'] if i['name'].endswith('.jpg')]
        objects = np.zeros((54, 3), np.float32)
        objects[:, :2] = np.mgrid[0:9, 0:6].T.reshape(-1, 2)
        corners = {}
        for name in names:
            image = cv2.imread(str(source / name), cv2.IMREAD_GRAYSCALE)
            assert image.shape == (480, 640), name
            found, points = cv2.findChessboardCorners(image, (9, 6))
            assert found, name
            corners[name] = cv2.cornerSubPix(image, points, (11, 11), (-1, -1),
                (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 100, 1e-5)).reshape(-1, 2)
        train = [n for n in names if n not in HELD_OUT]
        rms, K, distortion, _, _ = cv2.calibrateCamera(
            [objects] * len(train), [corners[n] for n in train], (640, 480), None, None)
        report.update(trainingViews=train, heldOutViews=HELD_OUT, trainingRms=rms,
                      K=K.tolist(), distortion=distortion.flatten().tolist(),
                      objectPoints=objects.tolist(), measuredCorners={n:p.tolist() for n,p in corners.items()})
        fit = np.array([(x + y) % 2 == 0 for y in range(6) for x in range(9)])
        for name in HELD_OUT:
            ok, rvec, tvec = cv2.solvePnP(objects[fit], corners[name][fit], K, distortion,
                                        flags=cv2.SOLVEPNP_ITERATIVE)
            assert ok, name
            projected, _ = cv2.projectPoints(objects, rvec, tvec, K, distortion)
            projected = projected.reshape(-1, 2)
            error = np.linalg.norm(projected - corners[name], axis=1)
            validation = error[~fit]
            rotation, _ = cv2.Rodrigues(rvec)
            row = dict(name=name, poseFitIndices=np.flatnonzero(fit).tolist(),
                       validationIndices=np.flatnonzero(~fit).tolist(),
                       worldToCameraRotation=rotation.tolist(), worldToCameraTranslation=tvec.flatten().tolist(),
                       projected=projected.tolist(), heldOutRms=float(np.sqrt(np.mean(validation**2))),
                       heldOutMax=float(validation.max()), heldOutP95=float(np.percentile(validation, 95)))
            report['views'].append(row)
            assert row['heldOutRms'] <= report['limits']['heldOutRmsPixels'], row
            assert row['heldOutMax'] <= report['limits']['heldOutMaxPixels'], row
        # Independent library inversion at image boundaries and interior points.
        points = np.array([[x, y] for y in np.linspace(0,480,13) for x in np.linspace(0,640,17)], np.float64)
        ideal = cv2.undistortPoints(points.reshape(-1,1,2), K, distortion, P=K,
            criteria=(cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 100, 1e-13)).reshape(-1,2)
        assert np.isfinite(ideal).all()
        report['inverseOracle'] = [dict(distorted=a.tolist(), ideal=b.tolist()) for a,b in zip(points,ideal)]
        report['status'] = 'accepted_external_calibration_native_rendering_pending'
    except BaseException as cause:
        report.update(status='failed', failure=repr(cause))
        raise
    finally:
        (output / 'calibration.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(dict(status=report['status'], trainingRms=rms,
        heldOut=[{k:v[k] for k in ('name','heldOutRms','heldOutP95','heldOutMax')} for v in report['views']])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--download', action='store_true', help='Fetch only pinned, hash-verified official sample bytes')
    args = parser.parse_args()
    calibration(args.source, args.output, args.download)
