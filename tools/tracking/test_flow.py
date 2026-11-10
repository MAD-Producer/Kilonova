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

    def test_plane_seed_windows_stay_inside_selected_plane_after_manual_reseed(self):
        seed=dict(kind='plane',roi=[24.,24.,128.,104.])
        tracker=flow.Tracker(seed,self.texture)
        matrices=[np.eye(3),np.array([[1.,.08,2.],[.02,.9,4.],[.0001,0.,1.]])]
        radius=flow.LK_WINDOW//2+1+1+flow.LIMITS['prefilterKernel']//2
        for matrix in matrices:
            with self.subTest(matrix=matrix.tolist()):
                tracker.reseed(self.texture,matrix)
                self.assertGreaterEqual(len(tracker.points),flow.LIMITS['minimumFeatures'])
                for x,y in tracker.points.astype(int):
                    patch=tracker.reference_roi_mask[y-radius:y+radius+1,x-radius:x+radius+1]
                    self.assertEqual(patch.shape,(2*radius+1,)*2)
                    self.assertTrue(np.all(patch==255))

    def test_plane_with_texture_only_at_boundary_refuses_instead_of_using_outside_support(self):
        image=np.full((160,192),128,np.uint8)
        noise=self.rng.integers(20,235,(104,128),np.uint8)
        noise[9:-9,9:-9]=128
        image[24:128,24:152]=noise
        image=flow.prepare(image)
        seed=dict(kind='plane',roi=[24.,24.,128.,104.])
        # Verify this is textured in the selected region; refusal must come
        # from window support rather than an accidentally empty control.
        centres=np.zeros_like(image);centres[24:128,24:152]=255
        found=flow.cv2.goodFeaturesToTrack(image,200,.01,5,mask=centres,blockSize=5)
        self.assertIsNotNone(found)
        self.assertGreaterEqual(len(found),flow.LIMITS['minimumFeatures'])
        tracker=flow.Tracker(seed,image)
        self.assertIsNone(tracker.initial)
        self.assertFalse(tracker.active)
        self.assertIn('too few source features',tracker.reason)

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

    def test_plane_reference_compensates_perspective_without_accumulated_patch_bias(self):
        rng=np.random.default_rng(8891)
        image=flow.prepare(rng.integers(20,236,(192,256),np.uint8))
        seed=dict(kind='plane',roi=[48.,40.,160.,112.])
        probes=[[48.,40.],[208.,40.],[208.,152.],[48.,152.],[128.,96.]]
        center=np.array([[1.,0.,-128.],[0.,1.,-96.],[0.,0.,1.]])

        def matrix(index):
            angle=.003*index
            scale=1.+.002*index
            local=np.array([[scale*np.cos(angle),-scale*np.sin(angle),.08*index],
                [scale*np.sin(angle),scale*np.cos(angle),.03*index],
                [.000005*index,-.000002*index,1.]])
            return np.linalg.inv(center)@local@center

        # Both an original seed and a later explicit correction must retain
        # the original coordinate domain. No truth is given after that edit.
        for start in (0,20):
            with self.subTest(manual_reference=start):
                previous=flow.cv2.warpPerspective(image,matrix(start),(256,192))
                tracker=flow.Tracker(seed,image)
                if start:
                    tracker.correct(previous,dict(visibility='visible',
                        observation=dict(kind='plane',matrix=matrix(start).tolist())))
                for index in range(start+1,61):
                    current=flow.cv2.warpPerspective(image,matrix(index),(256,192))
                    observation,_=tracker.observe(previous,current)
                    self.assertIsNotNone(observation,tracker.reason)
                    errors=np.linalg.norm(flow.transform(np.array(observation['matrix']),probes)
                        -flow.transform(matrix(index),probes),axis=1)
                    self.assertLess(float(errors.max()),.1)
                    previous=current

    def test_small_heldout_residual_does_not_prove_plane_edge_extrapolation(self):
        reference=np.array([[x,y] for x in (40.,80.,120.,160.,200.) for y in (78.,80.,82.)])
        center=np.array([[1.,0.,-120.],[0.,1.,-80.],[0.,0.,1.]])
        independent=np.linalg.inv(center)@np.array([[1.,0.,0.],[0.,1.,0.],[0.,.001,1.]])@center
        points=flow.transform(independent,reference)
        residual=np.linalg.norm(reference-points,axis=1)
        self.assertLess(float(residual.max()),flow.LIMITS['heldOutMaxPixels'])
        self.assertLess(float(np.sqrt(np.mean(residual**2))),flow.LIMITS['heldOutRmsPixels'])
        with self.assertRaisesRegex(ValueError,'independent extrapolation disagreement'):
            flow.check_plane_extrapolation(np.eye(3),reference,points,[24.,24.,184.,128.])
        flow.check_plane_extrapolation(independent,reference,points,[24.,24.,184.,128.])

    def test_plane_reference_requires_real_pixels_across_the_entire_lk_window(self):
        seed=dict(kind='plane',roi=[24.,24.,128.,104.])
        tracker=flow.Tracker(seed,self.texture)
        # Severe clipping cannot be made visible by the zero padding introduced
        # while rectifying the current image back into reference coordinates.
        clipped=flow.cv2.warpAffine(self.texture,np.array([[1.,0.,170.],[0.,1.,0.]]),(192,160))
        self.assertIsNone(tracker.observe(self.texture,clipped)[0])
        self.assertFalse(tracker.active)


if __name__=='__main__':
    unittest.main()
