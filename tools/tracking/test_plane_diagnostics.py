"""Actual numeric rejection controls for bounded planar failure causes."""
import unittest
import numpy as np
import plate_flow as flow


class PlaneDiagnostics(unittest.TestCase):
    def test_real_robust_fit_rejection_identifies_consensus_without_overriding_it(self):
        flow.cv2.setRNGSeed(88142)
        rng = np.random.default_rng(88142)
        reference = rng.uniform(20, 180, (20, 2))
        truth = np.array([[1., .02, 3.], [-.01, 1., 5.], [.0001, -.0001, 1.]])
        points = flow.transform(truth, reference)
        points[12:] = rng.uniform(400, 800, (8, 2))
        matrix, inliers = flow.cv2.findHomography(reference, points, flow.cv2.RANSAC,
            flow.LIMITS['planeFitPixels'], maxIters=2000, confidence=.995)
        self.assertIsNotNone(matrix)
        self.assertEqual(int(inliers.sum()), 12)
        # Heldouts are exact and cannot excuse insufficient fit consensus.
        held = flow.transform(truth, [[30.,30.],[140.,30.],[140.,140.],[30.,140.]])
        errors = np.linalg.norm(flow.transform(matrix, [[30.,30.],[140.,30.],[140.,140.],[30.,140.]])-held,axis=1)
        with self.assertRaisesRegex(ValueError,
                r'plane fit has too few inliers: inliers=12 fit=20 ratio=0\.6 minimum_count=12 minimum_ratio=0\.7') as caught:
            flow.check_plane_residuals(inliers, errors)
        self.assertLess(len(str(caught.exception)), 256)

    def test_independent_residual_gates_report_measurements_and_keep_inclusive_limits(self):
        inliers = np.ones((20,1), np.uint8)
        cases = [('rms_px=1.6 maximum_px=1.6', np.full(16,1.6)),
                 ('rms_px=0.775 maximum_px=3.1', np.array([3.1]+[0.]*15))]
        for message, errors in cases:
            with self.subTest(message=message):
                with self.assertRaisesRegex(ValueError, message) as caught:
                    flow.check_plane_residuals(inliers, errors)
                self.assertIn('heldout=16', str(caught.exception))
                self.assertIn('rms_limit_px=1.5 maximum_limit_px=3', str(caught.exception))
                self.assertLess(len(str(caught.exception)),256)
        # The original <= gates admit equality for both RMS and maximum.
        errors = np.array([3.,0.,0.,0.])
        ratio, rms = flow.check_plane_residuals(inliers,errors)
        self.assertEqual((ratio,rms),(1.,1.5))
        with self.assertRaisesRegex(ValueError, 'plane held-out residual exceeds limit'):
            flow.check_plane_residuals(inliers,np.array([float('nan')]))


if __name__ == '__main__':
    unittest.main()
