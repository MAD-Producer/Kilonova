"""Validate original encoder words and exact offline sample identities.

This validates a recorded protocol. Synthetic tests of it are not native GPU
acceptance, and command intervals must never be summed into exclusive time.
"""
import math


REQUEST_DOMAIN = "offline_case_sample_index_plus_one_not_engine_worker_request"


def require(condition, reason):
    if not condition:
        raise ValueError("Command trace: " + reason)


def uint(value, field):
    require(type(value) is int and 0 <= value < 2**64, "invalid integer " + field)
    return value


def distribution(values):
    values = sorted(values)
    return {"count": len(values), **{key: values[math.ceil(len(values) * p) - 1]
            for key, p in (("p50", .5), ("p95", .95), ("p99", .99))}, "max": values[-1]}


def validate_command_trace(report):
    require(report["commandRequestDomain"] == REQUEST_DOMAIN, "offline caller mislabeled as a worker request")
    supported = report["commandTimestampSupported"]
    require(type(supported) is bool, "support must be explicit")
    frames = report["frames"]
    if not supported:
        require(report["gpuCommands"] is None and report["gpuCommandTrace"] is None,
                "unavailable native intervals must remain unknown")
        require(all(frame["mainCommand"] is None and frame["commandTimestampReadbackBytes"] == 0
                    for frame in frames), "unavailable command identities cannot imply samples")
        return {"available": False, "nativeCommands": None, "completeGpuCoverage": False}

    snapshot, trace = report["gpuCommands"], report["gpuCommandTrace"]
    epoch = uint(trace["epoch"], "epoch")
    require(epoch > 0 and snapshot["commands"]["epoch"] == epoch, "profiler epoch mismatch")
    period = snapshot["timestampPeriodNs"]
    require(type(period) in (int, float) and math.isfinite(period) and period > 0,
            "missing native timestamp period")
    require(trace["maxCommands"] == 8192 and trace["maxMetadataBytes"] == 16 * 1024 * 1024
            and 0 < trace["retainedMetadataBytes"] <= trace["maxMetadataBytes"], "production trace limits changed")
    require(trace["startedAfterCommand"] == 0 and trace["excludedActiveAtStart"] == 0
            and trace["active"] == [] and snapshot["commands"]["active"] == [],
            "creation window is incomplete or contains unresolved work")
    require(trace["truncated"] is False and trace["omittedRecords"] == 0
            and trace["firstOmittedCommand"] is None and trace["lastOmittedCommand"] is None
            and trace["counterSaturated"] is False and trace["observationFault"] is False
            and snapshot["commands"]["observationFault"] is False and snapshot["quarantinedSlots"] == 0,
            "omitted, truncated, saturated or faulty observations")
    records = trace["records"]
    for key in ("startedAfterCommand", "excludedActiveAtStart", "startedCommands", "stoppedAtCommand",
                "retainedMetadataBytes", "maxCommands", "maxMetadataBytes", "omittedRecords"):
        uint(trace[key], key)
    require(0 < len(records) <= trace["maxCommands"] and len(records) == trace["startedCommands"]
            == trace["stoppedAtCommand"], "terminal recording does not cover its creation window")
    counts = snapshot["commands"]["counts"]
    for key, value in counts.items():
        if key != "saturated":
            uint(value, key)
    require(counts["started"] == counts["completed"] == len(records)
            and snapshot["commands"]["consecutiveReadbackFailures"] == 0 and counts["saturated"] is False,
            "native completion counts disagree")
    for key in ("omittedBusy", "omittedSlots", "omittedInvalidContext", "omittedIdentityExhausted",
                "omittedContexts", "omittedExposureScopes", "omittedInputIdentifiers", "discarded",
                "unconfirmed", "mapFailed", "cancelled", "invalid", "omittedLogEvents"):
        require(counts[key] == 0, "nonzero fault or omission: " + key)
    readback = snapshot["readback"]
    for key, value in readback.items():
        if key != "countersSaturated":
            uint(value, key)
    for key in ("encodedResolveCalls", "mapRequests", "mapSuccesses", "validSamples"):
        require(readback[key] == len(records), "native callback count: " + key)
    for key in ("encodedResolveBytes", "encodedCopyBytes", "mappedBufferBytes"):
        require(readback[key] == 16 * len(records), "native word byte count: " + key)
    require(readback["mapFailures"] == readback["invalidSamples"] == 0
            and readback["countersSaturated"] is False, "native callback failed")

    by_request = {index + 1: [] for index in range(len(frames))}
    by_identity = {}
    stage_counts = {}
    for record in records:
        identity = (uint(record["epoch"], "record epoch"), uint(record["id"], "command id"))
        require(identity[0] == epoch and identity[1] > 0 and identity not in by_identity,
                "duplicate or wrong-epoch command identity")
        by_identity[identity] = record
        require(record["phase"] == "complete" and record["cause"] is None
                and record["exposure"] is None and record["input"] is None,
                "unknown or failed offline native command")
        words = record["timestampWords"]
        require(type(words) is list and len(words) == 2, "missing original timestamp words")
        begin, end = (uint(word, "timestamp word") for word in words)
        require(0 < begin < end, "invalid original timestamp ordering")
        elapsed = record["gpuElapsedUs"]
        require(type(elapsed) in (int, float) and math.isfinite(elapsed) and elapsed > 0
                and math.isclose(elapsed, (end - begin) * period / 1000, rel_tol=1e-12, abs_tol=1e-8),
                "duration disagrees with original words and native period")
        uint(record["elapsedUs"], "observation wall elapsed")
        require(len(record["contexts"]) == 1 and record["omittedContexts"] == 0,
                "ambiguous serialized offline context")
        context = record["contexts"][0]
        request = uint(context["request"], "offline request")
        require(request in by_request and context["operation"] == "render"
                and context["exposureRequest"] is None and context["exposurePoll"] is None,
                "wrong offline sample context")
        by_request[request].append(record)
        stage = record["stage"]
        require(type(stage) is str and bool(stage), "missing encoder stage")
        stage_counts[stage] = stage_counts.get(stage, 0) + 1
    require(set(identity[1] for identity in by_identity) == set(range(1, len(records) + 1)),
            "creation identity gap hidden by completion order")

    measured_main = []
    all_main = set()
    for index, frame in enumerate(frames):
        request = index + 1
        require(frame["offlineRequest"] == request, "offline sample ordinal changed")
        window = frame["allocationFrame"]
        window_id = uint(window["id"], "allocation frame")
        for key in ("request", "pendingSubmissions", "unconfirmedSubmissions", "submissions"):
            uint(window[key], key)
        require(window_id > 0 and window["request"] == request and window["operation"] == "render"
                and window["sourceFrame"] == frame["compositionFrame"] and window["outcome"] == "rendered"
                and window["pendingSubmissions"] == window["unconfirmedSubmissions"] == 0
                and window["submissions"] >= 1 and window["endAdmittedBytes"] is not None,
                "allocation window is incomplete or belongs to another sample")
        identity = frame["mainCommand"]
        require(type(identity) is dict, "missing exact main encoder identity")
        identity = (uint(identity["epoch"], "main epoch"), uint(identity["id"], "main id"))
        require(identity in by_identity and identity not in all_main, "main encoder missing or reused across samples")
        all_main.add(identity)
        main = by_identity[identity]
        require(main["stage"] == "composition_and_presentation" and main in by_request[request],
                "exact main encoder belongs to another caller")
        records_here = by_request[request]
        for record in records_here:
            context = record["contexts"][0]
            uint(context["allocationFrame"], "context allocation frame")
            require(context["allocationFrame"] == window_id
                    and context["sourceFrame"] == frame["compositionFrame"],
                    "allocation identity or authored frame differs from actual caller")
        require(frame["commandTimestampReadbackBytes"] == 16 * len(records_here),
                "per-sample readback bytes disagree with original records")
        if report["case"] == "four-4k-yuv":
            require(sum(record["stage"] == "media_preparation" for record in records_here) >= 4,
                    "four native conversion intervals are missing outside the main pass")
        if index >= 8:
            measured_main.append(main["gpuElapsedUs"])
    if report["case"] == "static":
        # Vector raster and MSAA resolve are inside the main encoder, before its
        # root pass. They are not a separate queue submission or exclusive span.
        vector = frames[0]["vectorUpload"]
        require(uint(vector["encodedUploadBytesTotal"], "vector encoded bytes") > 0
                and uint(vector["completedUploadBytesTotal"], "vector completed bytes") > 0,
                "actual vector preparation uploads missing inside the main encoder")
    if report["case"] == "nested-matte":
        require(stage_counts.get("nested_composition", 0) > 0, "native nested preparation interval missing")
    return {"available": True, "nativeCommands": len(records), "perStageCommandCounts": stage_counts,
            "measuredMainEncoderIntervalUs": distribution(measured_main),
            "scope": "Original per-encoder intervals; overlapping intervals are never summed into exclusive GPU time",
            "completeGpuCoverage": False}
