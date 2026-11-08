"""Analytic sensor-depth oracles, independent of any dataset or learned model."""
import math
import unittest

import depth_reference_metrics as metrics


class SensorDepthMetrics(unittest.TestCase):
    def test_known_affine_outlier_missing_and_invalid_controls(self):
        result = metrics.controls()
        self.assertEqual(result['invalid_controls'], 7)

    def test_one_extreme_sensor_pixel_cannot_shrink_the_error_units(self):
        reference = [1, 2000, 4000, 5000, 10000]
        prediction = [(5000 / value - 0.5) / 2 for value in reference]
        calibration = metrics.fit([(prediction, reference)] * 8)
        self.assertEqual(calibration.reference_scale_inverse_metres, 1.0)
        shifted = metrics.score(calibration, [v + 0.2 for v in prediction], reference)
        self.assertAlmostEqual(shifted['normalized_rms'], 0.4, places=9)
        self.assertAlmostEqual(shifted['normalized_maximum'], 0.4, places=9)

    def test_aggregate_is_weighted_by_valid_pixels_not_frame_count(self):
        calibration = metrics.Calibration(1.0, 0.0, 1.0, 2, 0)
        rows = [metrics.score(calibration, [2.0], [5000]),
                metrics.score(calibration, [1.0, 1.0, 1.0], [5000] * 3)]
        total = metrics.summarize(rows, calibration)
        self.assertEqual(total['valid'], 4)
        self.assertEqual(total['missing'], 0)
        self.assertEqual(total['mean_absolute_inverse_metres'], 0.25)
        self.assertEqual(total['normalized_rms'], 0.5)
        self.assertEqual(total['normalized_maximum'], 1.0)

    def test_missing_reference_never_conceals_nonfinite_prediction(self):
        calibration = metrics.Calibration(1.0, 0.0, 1.0, 2, 0)
        for prediction in (math.nan, math.inf, -math.inf):
            with self.subTest(prediction=prediction):
                with self.assertRaisesRegex(ValueError, 'nonfinite prediction'):
                    metrics.score(calibration, [1.0, prediction], [5000, 0])


if __name__ == '__main__':
    unittest.main()
