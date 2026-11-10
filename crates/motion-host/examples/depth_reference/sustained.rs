//! Bounded complete cohorts in one measured worker and one retained GPU service.
use super::*;

const MINIMUM_SECONDS: u64 = 1800;
const MAXIMUM_COHORTS: usize = 64;

fn continue_after_complete(cohorts: usize, elapsed: Duration) -> Result<bool> {
    if !(1..=MAXIMUM_COHORTS).contains(&cohorts) {
        return Err("sustained schedule requires one to64 complete cohorts".into());
    }
    if elapsed >= Duration::from_secs(MINIMUM_SECONDS) {
        return Ok(false);
    }
    if cohorts == MAXIMUM_COHORTS {
        return Err("sustained cohort cap reached before minimum duration".into());
    }
    Ok(true)
}

fn publish(
    root: &Path,
    status: &str,
    cohorts: &[serde_json::Value],
    elapsed: Duration,
    cause: Option<&str>,
) -> Result<()> {
    if cohorts.len() > MAXIMUM_COHORTS {
        return Err("sustained evidence exceeds cohort bound".into());
    }
    let report = serde_json::json!({
        "schema":1,"status":status,"cohorts":cohorts,"cause":cause,
        "elapsed_ns":elapsed.as_nanos(),"minimum_seconds":MINIMUM_SECONDS,
        "maximum_cohorts":MAXIMUM_COHORTS,"samples_per_cohort":32,
        "scope":"One explicit measured worker and retained GPU service; complete repeated fixed32 inputs; no retries, warmup deletion, quality or thermal qualification"
    });
    let pending = root.join("sustained.pending.json");
    fs::write(&pending, serde_json::to_vec_pretty(&report)?)?;
    fs::rename(pending, root.join("sustained.json"))?;
    Ok(())
}

fn cohort(
    jobs: &mut Jobs,
    model: &DepthModel,
    plan: &[Spec],
    input_root: &Path,
    root: &Path,
    ordinal: usize,
) -> Result<serde_json::Value> {
    let mut profile = depth::capability::Profile::new(32)?;
    let mut rows = Vec::with_capacity(32);
    write_report(root, "running", &rows, None, None)?;
    let result: Result<serde_json::Value> = (|| {
        for row in plan {
            let loading = Instant::now();
            let rgb = read_rgb(input_root, row)?;
            let prepared = prepared_identity([640, 480], &rgb);
            let caller_rgb_load = loading.elapsed();
            let key = Key {
                source: DepthSource {
                    asset: 750_000 + row.index as u64,
                    revision: 1,
                    pts: DepthTime {
                        ticks: row.rgb_ns.try_into()?,
                        timescale: 1_000_000_000,
                    },
                },
                geometry: DepthGeometry {
                    source_size: [640, 480],
                    crop: [0, 0, 640, 480],
                    quarter_turns: 0,
                    mirror_x: false,
                },
                model: model.clone(),
            };
            let request = jobs.request(Input::new(key.clone(), [640, 480], Arc::from(rgb))?)?;
            let waiting = Instant::now();
            let ready = loop {
                if let Some(ready) = jobs.take_prepared_ready(&key)? {
                    break ready;
                }
                if waiting.elapsed() >= Duration::from_secs(60) {
                    log::error!(
                        "operation=depth_sustained cohort={ordinal} request={request} asset={} phase=wait elapsed_ms={} cause=request did not complete within60seconds",
                        key.source.asset,
                        waiting.elapsed().as_millis()
                    );
                    return Err("sustained request did not complete within60seconds".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            };
            let measurement = ready
                .measurement()
                .ok_or("sustained worker omitted measurement")?;
            let estimate = ready.estimate();
            if measurement.request() != request
                || measurement.execution().provider != estimate.provider
                || measurement.total() != estimate.costs.total
                || measurement.total() < measurement.resolve()
                || ready.input() != prepared
                || estimate.resource.values().len() != 640 * 480
            {
                return Err(format!(
                    "sustained identity mismatch cohort={ordinal} index={} request={request}",
                    row.index
                )
                .into());
            }
            profile.record(&ready)?;
            let writing = Instant::now();
            let values: Vec<u8> = estimate
                .resource
                .values()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            let filename = format!("{:02}.relative.f32le", row.index);
            use std::io::Write;
            fs::File::create_new(root.join(&filename))?.write_all(&values)?;
            let output_sha256 = hex(&Sha256::digest(&values));
            let caller_output = writing.elapsed();
            let c = &estimate.costs;
            rows.push(serde_json::json!({
                "cohort":ordinal,"index":row.index,"split":row.split,"key":key,"request":request,
                "input":ready.input(),"execution":measurement.execution(),
                "worker_total_ns":measurement.total().as_nanos(),"resolve_ns":measurement.resolve().as_nanos(),
                "retained_gpu_buffer_capacity_bytes":measurement.retained_gpu_buffer_capacity(),
                "inference_process_memory":measurement.inference_process_memory(),"process_memory":measurement.process_memory(),
                "adapter_memory_samples":c.adapter_memory_samples,
                "observed_resident_sum_bytes":measurement.process_memory().and_then(|m|m.observed_resident_sum()),
                "caller_rgb_load_ns":caller_rgb_load.as_nanos(),"caller_output_ns":caller_output.as_nanos(),
                "source_prepare_ns":c.source_prepare.as_nanos(),"source_rgb_bytes":c.source_rgb_bytes,
                "load_ns":c.load.as_nanos(),"preprocess_ns":c.preprocess.as_nanos(),"inference_ns":c.inference.as_nanos(),
                "postprocess_ns":c.postprocess.as_nanos(),"host_transport_ns":c.host_transport.as_nanos(),
                "input_copy_ns":c.input_copy.as_nanos(),"output_copy_ns":c.output_copy.as_nanos(),
                "input_tensor_bytes":c.input_bytes,"output_tensor_bytes":c.output_bytes,
                "output":{"filename":filename,"bytes":values.len(),"sha256":output_sha256}
            }));
            write_report(root, "running", &rows, None, None)?;
        }
        let counts = jobs.counts()?;
        let expected = ((ordinal + 1) * 32) as u64;
        if counts.submitted != expected
            || counts.completed != expected
            || counts.failed != 0
            || counts.canceled != 0
            || counts.cache_hits != 0
            || counts.cache_misses != 0
            || counts.restored != 0
            || jobs.in_flight()?.is_some()
            || jobs.queue_depth()? != 0
        {
            return Err("sustained cohort reused, discarded or retained work".into());
        }
        let summary = profile.summary()?;
        Ok(
            serde_json::json!({"samples":summary.samples(),"model":summary.model(),"execution":summary.execution(),
            "fastest_ns":summary.fastest().as_nanos(),"median_ns":summary.median().as_nanos(),"slowest_ns":summary.slowest().as_nanos(),
            "submitted":counts.submitted,"completed":counts.completed,"failed":counts.failed,"canceled":counts.canceled,
            "cache_hits":counts.cache_hits,"cache_misses":counts.cache_misses,"restored":counts.restored}),
        )
    })();
    match result {
        Ok(summary) => {
            write_report(root, "completed", &rows, Some(&summary), None)?;
            Ok(summary)
        }
        Err(primary) => {
            let cause: String = primary.to_string().chars().take(2048).collect();
            if let Err(secondary) = write_report(root, "failed", &rows, None, Some(&cause)) {
                log::error!(
                    "operation=depth_sustained cohort={ordinal} phase=failed_cohort_report primary={cause} secondary={secondary}"
                );
            }
            Err(primary)
        }
    }
}

pub(super) fn run(
    backend: impl Backend,
    model: DepthModel,
    input_root: &Path,
    root: &Path,
) -> Result<()> {
    let plan: Vec<Spec> =
        serde_json::from_slice(&bounded_read(&input_root.join("inputs.json"), 64 << 10)?)?;
    validate_plan(&plan)?;
    let mut jobs = Jobs::start_measured(backend)?;
    let began = Instant::now();
    let mut completed = Vec::with_capacity(MAXIMUM_COHORTS);
    publish(root, "running", &completed, began.elapsed(), None)?;
    let result: Result<()> = (|| {
        for ordinal in 0..MAXIMUM_COHORTS {
            log::info!("start operation=depth_sustained cohort={ordinal} phase=complete_fixed32");
            log::logger().flush();
            let directory = format!("cohort-{ordinal:02}");
            let output = root.join(&directory);
            fs::create_dir(&output)?;
            let summary = cohort(&mut jobs, &model, &plan, input_root, &output, ordinal)?;
            completed.push(
                serde_json::json!({"cohort":ordinal,"directory":directory,"summary":summary}),
            );
            publish(root, "running", &completed, began.elapsed(), None)?;
            if !continue_after_complete(completed.len(), began.elapsed())? {
                return Ok(());
            }
        }
        Err("sustained cohort cap reached before minimum duration".into())
    })();
    // Drop/reap before the final record. No next cohort can recreate a service
    // after a failure, and the caller can inspect actual workspace cleanup.
    drop(jobs);
    match result {
        Ok(()) => publish(root, "completed", &completed, began.elapsed(), None),
        Err(primary) => {
            let cause: String = primary.to_string().chars().take(2048).collect();
            if let Err(secondary) =
                publish(root, "failed", &completed, began.elapsed(), Some(&cause))
            {
                log::error!(
                    "operation=depth_sustained phase=failed_report primary={cause} secondary={secondary}"
                );
            }
            Err(primary)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedule_requires_complete_cohorts_duration_and_refuses_a_short_capped_run() {
        assert!(continue_after_complete(1, Duration::from_secs(1799)).unwrap());
        assert!(!continue_after_complete(1, Duration::from_secs(1800)).unwrap());
        assert!(!continue_after_complete(64, Duration::from_secs(1801)).unwrap());
        assert!(continue_after_complete(64, Duration::from_secs(1799))
            .unwrap_err()
            .to_string()
            .contains("cap reached"));
        assert!(continue_after_complete(0, Duration::from_secs(1800)).is_err());
        assert!(continue_after_complete(65, Duration::from_secs(1800)).is_err());
    }
}
