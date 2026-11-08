"""Independently authored fixed inverse-depth evaluation; no model/data IO.

Calibration uses all nonzero registered sensor pixels from the first eight
timestamp-selected pairs. Evaluation uses the following 24 pairs, without
refitting, residual trimming or per-frame normalization. Sensor units are
raw/5000 metres. This module does not establish sensor or model accuracy.
"""
from dataclasses import dataclass
import math


def valid_pairs(prediction, sensor):
    if len(prediction) != len(sensor) or not prediction:
        raise ValueError("equal nonempty registered planes required")
    for predicted, raw in zip(prediction, sensor):
        if type(raw) is not int or not 0 <= raw <= 65535:
            raise ValueError("sensor value must be an unsigned 16-bit integer")
        if not math.isfinite(predicted):
            raise ValueError("nonfinite prediction; missing sensor is not permission to hide it")
        if raw:
            yield predicted, 5000.0 / raw


@dataclass(frozen=True)
class Calibration:
    slope: float
    offset: float
    reference_scale_inverse_metres: float
    samples: int
    missing: int


def fit(frames):
    """One affine calibration, using stable online centred covariance."""
    n = missing = 0
    mean_x = mean_y = covariance = variance = 0.0
    for prediction, sensor in frames:
        count = 0
        for x, y in valid_pairs(prediction, sensor):
            count += 1
            n += 1
            dx, dy = x - mean_x, y - mean_y
            mean_x += dx / n
            mean_y += dy / n
            variance += dx * (x - mean_x)
            covariance += dx * (y - mean_y)
        missing += len(sensor) - count
    if n < 2 or variance <= 0:
        raise ValueError("calibration requires nondegenerate prediction and reference")
    slope = covariance / variance
    offset = mean_y - slope * mean_x
    if not all(map(math.isfinite, (slope, offset))) or slope <= 0:
        raise ValueError("increasing relative depth must fit finite positive inverse-depth slope")
    # A fixed physical scale cannot be inflated by one extreme sensor sample.
    # This is not a per-scene range, per-frame range or fitted model statistic.
    return Calibration(slope, offset, 1.0, n, missing)


def score(calibration, prediction, sensor):
    """All valid sensor samples; normalization is exactly one inverse metre."""
    n = 0
    squared = absolute = maximum = 0.0
    for x, y in valid_pairs(prediction, sensor):
        residual = calibration.slope * x + calibration.offset - y
        if not math.isfinite(residual):
            raise ValueError("calibrated residual is nonfinite")
        n += 1
        squared += residual * residual
        absolute += abs(residual)
        maximum = max(maximum, abs(residual))
    if not n or not math.isfinite(squared):
        raise ValueError("evaluation has no valid samples or nonfinite aggregate")
    span = calibration.reference_scale_inverse_metres
    return dict(valid=n, missing=len(sensor) - n,
                sum_squared_inverse_metres=squared,
                mean_absolute_inverse_metres=absolute / n,
                rms_inverse_metres=math.sqrt(squared / n),
                maximum_inverse_metres=maximum,
                normalized_rms=math.sqrt(squared / n) / span,
                normalized_maximum=maximum / span)


def summarize(rows, calibration):
    if not rows:
        raise ValueError("evaluation requires frames")
    n = sum(row['valid'] for row in rows)
    squared = math.fsum(row['sum_squared_inverse_metres'] for row in rows)
    absolute = math.fsum(row['mean_absolute_inverse_metres'] * row['valid'] for row in rows)
    maximum = max(row['maximum_inverse_metres'] for row in rows)
    span = calibration.reference_scale_inverse_metres
    return dict(frames=len(rows), valid=n, missing=sum(row['missing'] for row in rows),
                mean_absolute_inverse_metres=absolute / n,
                rms_inverse_metres=math.sqrt(squared / n), maximum_inverse_metres=maximum,
                normalized_rms=math.sqrt(squared / n) / span,
                normalized_maximum=maximum / span)


def _require(condition, message):
    if not condition:
        raise AssertionError(message)


def controls():
    # Known independently constructed y=2*x+0.5 inverse metres. Zero remains
    # missing; its prediction never influences fit or score.
    sensor = [2000, 4000, 5000, 10000, 0]
    prediction = [(5000 / v - 0.5) / 2 if v else 999.0 for v in sensor]
    fitted = fit([(prediction, sensor)] * 8)
    _require(math.isclose(fitted.slope, 2, rel_tol=1e-14), 'analytic control failed: math.isclose(fitted.slope, 2, rel_tol=1e-14)')
    _require(math.isclose(fitted.offset, 0.5, abs_tol=1e-14), 'analytic control failed: math.isclose(fitted.offset, 0.5, abs_tol=1e-14)')
    _require(fitted.samples == 32 and fitted.missing == 8, 'analytic control failed: fitted.samples == 32 and fitted.missing == 8')
    perfect = score(fitted, prediction, sensor)
    _require(perfect['valid'] == 4 and perfect['missing'] == 1, "analytic control failed: perfect['valid'] == 4 and perfect['missing'] == 1")
    _require(perfect['normalized_maximum'] < 1e-14, "analytic control failed: perfect['normalized_maximum'] < 1e-14")
    shifted = score(fitted, [v + 0.2 for v in prediction], sensor)
    _require(math.isclose(shifted['rms_inverse_metres'], 0.4, rel_tol=1e-13), "analytic control failed: math.isclose(shifted['rms_inverse_metres'], 0.4, rel_tol=1e-13)")
    _require(math.isclose(shifted['normalized_rms'], 0.4, rel_tol=1e-13), "analytic control failed: math.isclose(shifted['normalized_rms'], 0.4, rel_tol=1e-13)")
    _require(shifted['normalized_maximum'] > 0.19, "analytic control failed: shifted['normalized_maximum'] > 0.19")  # No evaluation-frame fit.
    # One large outlier is retained and dominates the maximum. No clipping.
    outlier = prediction.copy()
    outlier[0] += 20
    row = score(fitted, outlier, sensor)
    _require(math.isclose(row['maximum_inverse_metres'], 40, rel_tol=1e-13), "analytic control failed: math.isclose(row['maximum_inverse_metres'], 40, rel_tol=1e-13)")
    _require(math.isclose(row['normalized_maximum'], 40, rel_tol=1e-13), "analytic control failed: math.isclose(row['normalized_maximum'], 40, rel_tol=1e-13)")
    total = summarize([perfect, shifted, row], fitted)
    _require(total['frames'] == 3 and total['valid'] == 12 and total['missing'] == 3, "analytic control failed: total['frames'] == 3 and total['valid'] == 12 and (total['missing'] == 3)")
    _require(math.isclose(total['rms_inverse_metres'], math.sqrt((0.64 + 1600) / 12), rel_tol=1e-13), "analytic control failed: math.isclose(total['rms_inverse_metres'], math.sqrt((0.64 + 1600) / 12), rel_tol=1e-13)")
    rejected = 0
    for action in (
        lambda: fit([([1, 1], [2000, 4000])]),
        lambda: fit([([-1, 0], [2000, 4000])]),
        lambda: fit([([1, 2], [0, 0])]),
        lambda: score(fitted, [1], [2000, 4000]),
        lambda: score(fitted, [1, float('nan')], [2000, 0]),
        lambda: score(fitted, [1], [-1]),
        lambda: score(fitted, [1], [0]),
    ):
        try:
            action()
        except ValueError:
            rejected += 1
        else:
            raise AssertionError("invalid input accepted")
    _require(rejected == 7, 'analytic control failed: rejected == 7')
    return dict(status='analytic_controls_passed_no_dataset_or_model_predictions',
                invalid_controls=rejected,
                scope='Known affine, training-only fit, retained outlier, missing sensor and invalid-input controls')


if __name__ == '__main__':
    import json
    print(json.dumps(controls(), sort_keys=True))
