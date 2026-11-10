use super::*;
use motion_model::depth::DepthAxis;

struct Physical {
    bias: f64,
}
impl Backend for Physical {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> Result<Estimate, Error> {
        let candidate = input.key().model.processing == "candidate";
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: [20, 1],
                    source: input.key().source.clone(),
                    geometry: input.key().geometry.clone(),
                    model: input.key().model.clone(),
                    measure: DepthMeasure::Metric {
                        meters_per_unit: if candidate { 0.5 } else { 1. },
                        axis: DepthAxis::CameraZ,
                    },
                    scale: if candidate { 2. } else { 1. },
                    offset: if candidate { self.bias } else { 0. },
                    valid_range: None,
                },
                (1..=20).map(|i| i as f32).collect(),
                None,
                None,
            )?,
            provider: "analytic-physical-control".into(),
            costs: Costs::default(),
        })
    }
}

#[test]
fn worker_admits_declared_unit_conversion_and_preserves_original_metric_bias_rejection() {
    for bias in [0., 2.] {
        let mut c = controller();
        let mut jobs = Jobs::start(Physical { bias }).unwrap();
        let (ticket, reference) = reference(&mut c, &mut jobs, 751);
        c.request_aligned(
            &ticket,
            &mut jobs,
            input(ticket.key(), 1),
            &reference,
            limits(),
        )
        .unwrap()
        .unwrap();
        completed(&jobs, 2);
        let result = c.take_aligned_ready(&ticket, &mut jobs, ticket.key());
        if bias == 0. {
            let admitted = result.unwrap().unwrap();
            let view = admitted.view(&c, ticket.key()).unwrap().unwrap();
            assert_eq!(
                view.sample_measure(),
                reference.estimate().resource.descriptor().measure
            );
            assert_ne!(view.sample_measure(), view.descriptor().measure);
            for i in 0..20 {
                assert_eq!(
                    view.sample(i).unwrap(),
                    reference.estimate().resource.sample(i)
                );
            }
            assert_eq!(jobs.counts().unwrap().completed, 2);
        } else {
            assert!(matches!(
                result,
                Err(TransitionError::Job(Error::Alignment(
                    AdmissionError::Alignment(Rejection::Residual { .. })
                )))
            ));
            assert_eq!(jobs.counts().unwrap().failed, 1);
            assert!(jobs.take_ready(ticket.key()).unwrap().is_none());
        }
        assert!(jobs.in_flight().unwrap().is_none());
        assert_eq!(jobs.queue_depth().unwrap(), 0);
    }
}
