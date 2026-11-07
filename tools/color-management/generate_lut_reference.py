"""Regenerate independently authored Cube probes through pinned official OCIO.

The external reference runtime is never imported by the application or CI tests.
Run with a separately installed opencolorio==2.6.0 Python environment.
"""
import argparse
import array
import hashlib
import json
from pathlib import Path
import platform
import sys

import PyOpenColorIO as ocio


def generate(output):
    if ocio.__version__ != '2.6.0':
        raise RuntimeError(f'expected OpenColorIO 2.6.0, got {ocio.__version__}')
    output.mkdir(parents=True, exist_ok=True)
    minimum = [-1.0, -2.0, -0.5]
    maximum = [2.0, 1.0, 3.5]
    tables = {
        'one-d': (1, 5, [[2*x*x-0.125, 0.75*x-0.25, 2*x*x*x]
                          for x in (0., .25, .5, .75, 1.)]),
        'three-d': (3, 3, [[.1+r*r+.2*g*b, -.25+g*g+.3*r*b, 1.5*b*b+.15*r*g]
                            for b in (0., .5, 1.) for g in (0., .5, 1.) for r in (0., .5, 1.)]),
    }
    # Asymmetric domains and mixed fractional cells distinguish axis order,
    # tetrahedral/trilinear interpolation, clamping, negative values and HDR.
    normalized = [[0., 0., 0.], [1., 1., 1.], [-.4, 1.2, .35],
                  [.12, .34, .56], [.73, .14, .47], [.26, .86, .52],
                  [.91, .67, .23], [.5, .5, .5], [.001, .999, .41],
                  [.499, .501, .123], [.61, .22, .82], [.125, .375, .875]]
    cases = []
    config = ocio.Config.CreateRaw()
    for name, (dimension, size, values) in tables.items():
        text = (f'# Independently authored numerical fixture\nLUT_{dimension}D_SIZE {size}\n'
                + 'DOMAIN_MIN ' + ' '.join(map(str, minimum)) + '\n'
                + 'DOMAIN_MAX ' + ' '.join(map(str, maximum)) + '\n'
                + ''.join(' '.join(f'{v:.9g}' for v in row) + '\n' for row in values))
        path = output / (name + '.cube')
        path.write_bytes(text.encode('ascii'))
        methods = [('nearest', ocio.INTERP_NEAREST), ('linear', ocio.INTERP_LINEAR)]
        if dimension == 3:
            methods.append(('tetrahedral', ocio.INTERP_TETRAHEDRAL))
        for method, interpolation in methods:
            transform = ocio.FileTransform(src=str(path.resolve()), interpolation=interpolation)
            processor = config.getProcessor(transform).getDefaultCPUProcessor()
            reference_path = 'cube_file'
            if method == 'nearest':
                # OCIO's CPU LUT renderer resolves nearest to linear for both
                # dimensions. Keep its actual result, but use an independent
                # exhaustive grid-distance oracle for explicit nearest semantics.
                reference_path = 'nearest_grid_distance'
            for alpha_mode in ('straight', 'premultiplied'):
                inputs, expected, file_expected = [], [], []
                for index, point in enumerate(normalized):
                    rgb = [minimum[i] + point[i] * (maximum[i] - minimum[i]) for i in range(3)]
                    alpha = [1., .3, .01, 0.][index % 4]
                    original = [v*alpha for v in rgb] if alpha_mode == 'premultiplied' else rgb
                    # Round at the same public float32 pixel boundary as Rust.
                    pixel = list(array.array('f', original + [alpha]))
                    straight = ([0., 0., 0.] if pixel[3] == 0. else [v/pixel[3] for v in pixel[:3]]) if alpha_mode == 'premultiplied' else pixel[:3]
                    result = processor.applyRGBA(list(array.array('f', straight + [pixel[3]])))
                    file_result = list(result)
                    if method == 'nearest':
                        indices = [min(range(size), key=lambda k: (
                            abs(straight[c] - (minimum[c] + k * (maximum[c] - minimum[c]) / (size - 1))), -k))
                            for c in range(3)]
                        selected = ([values[indices[c]][c] for c in range(3)] if dimension == 1 else
                                    values[indices[0] + size * (indices[1] + size * indices[2])])
                        result = list(array.array('f', selected + [pixel[3]]))
                    if alpha_mode == 'premultiplied':
                        result[:3] = [v*pixel[3] for v in result[:3]]
                        file_result[:3] = [v*pixel[3] for v in file_result[:3]]
                    inputs.append(pixel)
                    expected.append(list(array.array('f', result)))
                    file_expected.append(list(array.array('f', file_result)))
                cases.append({'cube': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                              'interpolation': method, 'alpha': alpha_mode,
                              'reference_path': reference_path, 'input': inputs, 'expected': expected,
                              'original_file_expected': file_expected})
    runtime = Path(ocio.__file__)
    # __file__ may identify the package initializer; record the actual extension.
    native = next(runtime.parent.glob('PyOpenColorIO*.pyd'), None)
    if native is None:
        native = next(runtime.parent.glob('PyOpenColorIO*.so'), None)
    result = {'reference': 'OpenColorIO', 'version': ocio.__version__,
              'platform': platform.platform(), 'python': sys.version,
              'native_sha256': hashlib.sha256(native.read_bytes()).hexdigest() if native else None,
              'tolerance': 0.00001, 'cases': cases}
    (output / 'reference.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'cases': len(cases), 'pixels_per_case': len(normalized), 'output': str(output)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    generate(parser.parse_args().output)
