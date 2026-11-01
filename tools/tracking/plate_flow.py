"""Forward/backward LK and independently checked planar correspondences.

Optional external CV tooling, not imported by ordinary CI admission tests.
No fallback to camera motion, inferred timestamps, or synthesized visible data.
"""
import math
import cv2
import numpy as np

from plate_contract import require

LIMITS = dict(forwardBackwardPixels=1.0, photometricError=12.0,
              planeFitPixels=1.5, heldOutRmsPixels=1.5, heldOutMaxPixels=3.0,
              minimumFeatures=16, minimumFitInliers=12, minimumInlierRatio=0.7,
              maximumFeatures=200, prefilterSigmaPixels=1.0, prefilterKernel=5)


def prepare(gray):
    # LK's brightness model is unstable on subpixel-warped near-Nyquist detail.
    # Filter both frames before feature detection/flow, preserving source pixel
    # coordinates. This changes the measurement, not its acceptance thresholds.
    return cv2.GaussianBlur(gray,(LIMITS['prefilterKernel'],)*2,LIMITS['prefilterSigmaPixels'])


def transform(matrix, points):
    points = np.asarray(points, np.float64).reshape(-1, 2)
    homogeneous = np.column_stack((points, np.ones(len(points)))) @ matrix.T
    require(np.isfinite(homogeneous).all() and np.all(np.abs(homogeneous[:, 2]) > 1e-10), 'plane crosses projective infinity')
    return homogeneous[:, :2] / homogeneous[:, 2:]


def plane_matrix(value, roi):
    matrix = np.asarray(value, np.float64)
    require(matrix.shape == (3, 3) and np.isfinite(matrix).all(), 'invalid plane matrix')
    require(np.linalg.cond(matrix) < 1e10, 'ill-conditioned plane matrix')
    x, y, w, h = roi
    corners = np.array([[x,y], [x+w,y], [x+w,y+h], [x,y+h]], np.float64)
    denominator = np.column_stack((corners, np.ones(4))) @ matrix[2]
    require(np.all(denominator > 1e-10) or np.all(denominator < -1e-10), 'plane ROI crosses projective infinity')
    quad = transform(matrix, corners)
    edges = np.roll(quad, -1, axis=0) - quad
    cross = edges[:, 0]*np.roll(edges[:, 1], -1)-edges[:, 1]*np.roll(edges[:, 0], -1)
    require(np.all(cross > 1e-6) or np.all(cross < -1e-6), 'plane ROI folds or collapses')
    area = abs(np.sum(quad[:, 0]*np.roll(quad[:, 1], -1)-quad[:, 1]*np.roll(quad[:, 0], -1)))*0.5
    require(0.1 <= area/(w*h) <= 10., 'plane area change exceeds prototype budget')
    return matrix


def flow(previous, current, points):
    points = np.asarray(points, np.float32).reshape(-1, 1, 2)
    if len(points) == 0:
        return np.empty((0,2)), np.zeros(0, bool), np.zeros(0)
    options = dict(winSize=(21,21), maxLevel=3,
                   criteria=(cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT, 30, .01),
                   minEigThreshold=1e-4)
    moved, status, error = cv2.calcOpticalFlowPyrLK(previous, current, points, None, **options)
    if moved is None:
        return np.zeros((len(points),2)), np.zeros(len(points),bool), np.zeros(len(points))
    finite = np.isfinite(moved).all(axis=(1,2))
    safe = moved.copy()
    safe[~finite] = 0
    returned, backward, _ = cv2.calcOpticalFlowPyrLK(current, previous, safe, None, **options)
    if returned is None:
        return moved.reshape(-1,2), np.zeros(len(points),bool), np.zeros(len(points))
    distance = np.linalg.norm(returned.reshape(-1,2)-points.reshape(-1,2),axis=1)
    moved = moved.reshape(-1,2)
    width, height = current.shape[1], current.shape[0]
    error = error.reshape(-1)
    valid = ((status.reshape(-1) != 0) & (backward.reshape(-1) != 0) & finite
             & np.isfinite(distance) & np.isfinite(error)
             & (distance <= LIMITS['forwardBackwardPixels'])
             & (error <= LIMITS['photometricError'])
             & (moved[:,0] >= 0) & (moved[:,0] < width)
             & (moved[:,1] >= 0) & (moved[:,1] < height))
    score = np.zeros(len(points))
    score[valid] = np.exp(-distance[valid]**2)*np.exp(-error[valid]/12.)
    return moved, valid, score


class Tracker:
    def __init__(self, seed, gray):
        self.seed = seed
        self.active = True
        self.reason = None
        self.reference = self.points = self.holdout = None
        if seed['kind'] == 'point':
            self.points = np.array([seed['position']], np.float32)
            self.initial = dict(kind='point', pixels=list(seed['position']))
        else:
            self.initial = dict(kind='plane', matrix=np.eye(3).tolist())
            try:
                self.reseed(gray, np.eye(3))
            except ValueError as cause:
                self.active, self.reason, self.initial = False, str(cause), None

    def reseed(self, gray, matrix):
        x, y, w, h = self.seed['roi']
        quad = transform(matrix, [[x,y],[x+w,y],[x+w,y+h],[x,y+h]])
        mask = np.zeros(gray.shape, np.uint8)
        cv2.fillConvexPoly(mask, np.rint(quad).astype(np.int32), 255)
        found = cv2.goodFeaturesToTrack(gray, LIMITS['maximumFeatures'], .01, 5, mask=mask, blockSize=5)
        require(found is not None and len(found) >= LIMITS['minimumFeatures'], 'plane has too few source features')
        points = found.reshape(-1,2)
        reference = transform(np.linalg.inv(matrix), points)
        inside = ((reference[:,0] >= x) & (reference[:,0] <= x+w)
                  & (reference[:,1] >= y) & (reference[:,1] <= y+h))
        self.points, self.reference = points[inside], reference[inside]
        require(len(self.points) >= LIMITS['minimumFeatures'], 'plane has too few reference features')
        self.holdout = np.arange(len(self.points)) % 4 == 0

    def observe(self, previous, current):
        if not self.active:
            return None, 0.
        moved, valid, scores = flow(previous, current, self.points)
        if self.seed['kind'] == 'point':
            if not valid[0]:
                self.active, self.reason = False, 'point forward/backward or appearance check failed'
                return None, 0.
            self.points = moved
            return dict(kind='point', pixels=moved[0].astype(float).tolist()), float(scores[0])
        self.points, self.reference, self.holdout = moved[valid], self.reference[valid], self.holdout[valid]
        fit = ~self.holdout
        if np.count_nonzero(fit) < LIMITS['minimumFitInliers'] or np.count_nonzero(self.holdout) < 4:
            self.active, self.reason = False, 'plane lost fit or held-out features'
            return None, 0.
        matrix, inliers = cv2.findHomography(self.reference[fit], self.points[fit], cv2.RANSAC,
            LIMITS['planeFitPixels'], maxIters=2000, confidence=.995)
        if matrix is None or inliers is None:
            self.active, self.reason = False, 'plane RANSAC failed'
            return None, 0.
        try:
            matrix = plane_matrix(matrix, self.seed['roi'])
            errors = np.linalg.norm(transform(matrix, self.reference[self.holdout])-self.points[self.holdout], axis=1)
            ratio = float(np.mean(inliers))
            rms = float(np.sqrt(np.mean(errors**2)))
            require(int(np.sum(inliers)) >= LIMITS['minimumFitInliers'] and ratio >= LIMITS['minimumInlierRatio'], 'plane fit has too few inliers')
            require(rms <= LIMITS['heldOutRmsPixels'] and float(errors.max()) <= LIMITS['heldOutMaxPixels'], 'plane held-out residual exceeds limit')
        except ValueError as cause:
            self.active, self.reason = False, str(cause)
            return None, 0.
        return dict(kind='plane', matrix=matrix.tolist()), ratio*math.exp(-rms)

    def correct(self, gray, edit):
        self.reason = None
        self.active = edit['visibility'] == 'visible'
        if self.active:
            observation = edit['observation']
            if self.seed['kind'] == 'point':
                self.points = np.asarray([observation['pixels']], np.float32)
            else:
                matrix = plane_matrix(observation['matrix'], self.seed['roi'])
                try:
                    self.reseed(gray, matrix)
                except ValueError as cause:
                    # Keep the explicit authored correction, but never invent
                    # evidence for subsequent automatic observations.
                    self.active, self.reason = False, str(cause)
        return edit['observation'], 1. if edit['visibility'] == 'visible' else 0.
