"""Optional real-CV regression controls; run explicitly in the credited CV env."""
import unittest
import numpy as np
import plate_flow as flow


class FlowControls(unittest.TestCase):
    def setUp(self):
        flow.cv2.setNumThreads(1)
        self.rng=np.random.default_rng(88)
        self.texture=flow.prepare(self.rng.integers(20,235,(160,192),np.uint8))
        self.seed=dict(id='feature',kind='point',position=[80.,70.])

    def test_explicit_reference_tracks_analytic_translation_without_accumulated_drift(self):
        tracker=flow.Tracker(self.seed,self.texture)
        for index in range(1,21):
            def image(t):
                return flow.cv2.warpAffine(self.texture,np.array([[1.,0.,t*.3],[0.,1.,t*.2]]),(192,160))
            observation,_=tracker.observe(image(index-1),image(index))
            self.assertIsNotNone(observation)
            error=np.linalg.norm(np.array(observation['pixels'])-[80.+index*.3,70.+index*.2])
            self.assertLess(error,.2)

    def test_local_pair_agreement_cannot_override_original_reference_and_manual_edit_recovers(self):
        tracker=flow.Tracker(self.seed,self.texture)
        unrelated=flow.prepare(self.rng.integers(20,235,(160,192),np.uint8))
        observation,_=tracker.observe(unrelated,unrelated)
        self.assertIsNone(observation)
        self.assertFalse(tracker.active)
        self.assertIn('reference',tracker.reason)
        # Loss remains explicit even if a subsequent local pair is perfect.
        self.assertIsNone(tracker.observe(unrelated,unrelated)[0])
        edit=dict(visibility='visible',observation=dict(kind='point',pixels=[80.,70.]))
        tracker.correct(unrelated,edit)
        self.assertIsNotNone(tracker.observe(unrelated,unrelated)[0])

    def test_reference_geometry_does_not_disable_adjacent_appearance_rejection(self):
        point=np.array([[80.,70.]],np.float32)
        brighter=np.minimum(self.texture.astype(np.int16)+30,255).astype(np.uint8)
        _,local,_=flow.flow(self.texture,brighter,point)
        _,reference,_=flow.flow(self.texture,brighter,point,initial=point,check_appearance=False)
        self.assertFalse(local[0])
        self.assertTrue(reference[0])
        tracker=flow.Tracker(self.seed,self.texture)
        self.assertIsNone(tracker.observe(self.texture,brighter)[0])

    def test_plane_shared_local_drift_is_rejected_and_manual_reference_can_recover(self):
        seed=dict(id='plane',kind='plane',roi=[24.,24.,128.,104.])
        tracker=flow.Tracker(seed,self.texture)
        unrelated=flow.prepare(self.rng.integers(20,235,(160,192),np.uint8))
        self.assertIsNotNone(tracker.initial)
        self.assertIsNone(tracker.observe(unrelated,unrelated)[0])
        self.assertFalse(tracker.active)
        edit=dict(visibility='visible',observation=dict(kind='plane',matrix=np.eye(3).tolist()))
        tracker.correct(unrelated,edit)
        observation,_=tracker.observe(unrelated,unrelated)
        self.assertIsNotNone(observation)
        self.assertLess(np.max(np.abs(np.array(observation['matrix'])-np.eye(3))),.001)

    def test_reference_refinement_keeps_known_small_projective_motion(self):
        rng=np.random.default_rng(8817)
        texture=flow.cv2.GaussianBlur(rng.integers(20,236,(192,256),dtype=np.uint8),(3,3),.6)
        previous=flow.prepare(texture)
        tracker=flow.Tracker(self.seed,previous)
        for index in range(1,20):
            matrix=np.array([[1.+.0007*index,.0003*index,.45*index],
                [-.0003*index,1.+.0004*index,.18*index],[.000008*index,-.000005*index,1.]])
            current=flow.prepare(flow.cv2.warpPerspective(texture,matrix,(256,192)))
            observation,_=tracker.observe(previous,current)
            self.assertIsNotNone(observation)
            expected=flow.transform(matrix,[[80.,70.]])[0]
            self.assertLess(np.linalg.norm(np.array(observation['pixels'])-expected),.75)
            previous=current


if __name__=='__main__':
    unittest.main()
