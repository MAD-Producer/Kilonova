"""Protocol rejection tests only. These fixtures never count as native evidence."""
import copy
import unittest

import collect_desktop_performance as study
from desktop_performance_commands import REQUEST_DOMAIN
from test_ci_desktop_performance import SOURCE, fixture


def command_fixture(name="animated"):
    report = fixture(name)
    records = []
    for frame in report["frames"]:
        request = frame["index"] + 1
        frame.update(offlineRequest=request, allocationFrame={
            "id": request + 100, "request": request, "operation": "render",
            "sourceFrame": frame["compositionFrame"], "outcome": "rendered",
            "pendingSubmissions": 0, "unconfirmedSubmissions": 0, "submissions": 1,
            "endAdmittedBytes": 32})
        stages = ["composition_and_presentation"]
        if name == "four-4k-yuv":
            stages += ["media_preparation"] * 4
        if request == 1 and name == "nested-matte":
            stages += ["nested_composition"]
        if name == "static":
            frame["vectorUpload"] = {"encodedUploadBytesTotal": 128, "completedUploadBytesTotal": 128}
        for stage in stages:
            identity = len(records) + 1
            records.append({"epoch": 7, "id": identity, "stage": stage, "phase": "complete",
                            "cause": None, "exposure": None, "input": None, "elapsedUs": 10,
                            "timestampWords": [identity * 1000, identity * 1000 + 250], "gpuElapsedUs": .5,
                            "omittedContexts": 0, "contexts": [{"request": request,
                                "allocationFrame": request + 100, "operation": "render",
                                "sourceFrame": frame["compositionFrame"], "exposureRequest": None, "exposurePoll": None}]})
            if stage == "composition_and_presentation":
                frame["mainCommand"] = {"epoch": 7, "id": identity}
        frame["commandTimestampReadbackBytes"] = 16 * len(stages)
    counts = {key: 0 for key in ("omittedBusy", "omittedSlots", "omittedInvalidContext", "omittedIdentityExhausted",
        "omittedContexts", "omittedExposureScopes", "omittedInputIdentifiers", "discarded", "unconfirmed", "mapFailed",
        "cancelled", "invalid", "omittedLogEvents", "evictedRecent")}
    counts.update(started=len(records), completed=len(records), saturated=False)
    readback = {key: len(records) for key in ("encodedResolveCalls", "mapRequests", "mapSuccesses", "validSamples")}
    readback.update({key: len(records) * 16 for key in ("encodedResolveBytes", "encodedCopyBytes", "mappedBufferBytes")})
    readback.update(mapFailures=0, invalidSamples=0, countersSaturated=False)
    report.update(schemaVersion=3, commandRequestDomain=REQUEST_DOMAIN, commandTimestampSupported=True,
                  gpuCommands={"timestampPeriodNs": 2., "quarantinedSlots": 0, "readback": readback,
                               "commands": {"epoch": 7, "counts": counts, "consecutiveReadbackFailures": 0,
                                            "observationFault": False, "active": []}},
                  gpuCommandTrace={"epoch": 7, "maxCommands": 8192, "maxMetadataBytes": 16 * 1024 * 1024,
                      "retainedMetadataBytes": 8192 * 256, "startedAfterCommand": 0, "excludedActiveAtStart": 0,
                      "startedCommands": len(records), "stoppedAtCommand": len(records), "active": [],
                      "truncated": False, "omittedRecords": 0, "firstOmittedCommand": None, "lastOmittedCommand": None,
                      "counterSaturated": False, "observationFault": False, "records": records})
    return report


class DesktopPerformanceCommandTests(unittest.TestCase):
    def test_original_words_recompute_and_completion_order_does_not_define_sample_identity(self):
        for name in study.CASES:
            with self.subTest(case=name):
                report = command_fixture(name)
                report["gpuCommandTrace"]["records"].reverse()
                result = study.validate_case(report, name, SOURCE, 3)["commandEvidence"]
                self.assertTrue(result["available"])
                self.assertFalse(result["completeGpuCoverage"])
                self.assertEqual(result["measuredMainEncoderIntervalUs"]["p95"], .5)
                self.assertEqual(result["nativeCommands"], len(report["gpuCommandTrace"]["records"]))

    def test_wrong_original_words_missing_callbacks_and_hidden_omissions_are_rejected(self):
        changes = [
            lambda r: r["gpuCommands"].update(timestampPeriodNs=0),
            lambda r: r["gpuCommands"].update(timestampPeriodNs=float("nan")),
            lambda r: r["gpuCommandTrace"]["records"][0].update(timestampWords=[0, 0]),
            lambda r: r["gpuCommandTrace"]["records"][0].update(timestampWords=[1250, 1000]),
            lambda r: r["gpuCommandTrace"]["records"][0].update(gpuElapsedUs=5.),
            lambda r: r["gpuCommandTrace"]["records"][0].update(epoch=8),
            lambda r: r["gpuCommandTrace"]["records"][1].update(id=1),
            lambda r: r["gpuCommandTrace"].update(truncated=True),
            lambda r: r["gpuCommandTrace"].update(omittedRecords=1),
            lambda r: r["gpuCommandTrace"].update(startedCommands=12),
            lambda r: r["gpuCommandTrace"].update(active=[{}]),
            lambda r: r["gpuCommands"]["readback"].update(mapSuccesses=0),
            lambda r: r["gpuCommands"]["readback"].update(mappedBufferBytes=0),
            lambda r: r["gpuCommands"]["readback"].update(invalidSamples=1),
            lambda r: r["gpuCommands"]["commands"]["counts"].update(omittedBusy=True),
            lambda r: r["gpuCommands"]["commands"]["counts"].update(omittedContexts=1),
        ]
        for index, mutate in enumerate(changes):
            with self.subTest(mutation=index):
                report = command_fixture()
                mutate(report)
                with self.assertRaisesRegex(ValueError, "Command trace"):
                    study.validate_case(report, "animated", SOURCE, 3)

    def test_same_authored_frame_cannot_join_the_wrong_main_encoder_or_allowance_window(self):
        changes = [
            lambda r: r.update(commandRequestDomain="engine_worker_request"),
            lambda r: r["frames"][1].update(mainCommand=copy.deepcopy(r["frames"][0]["mainCommand"])),
            lambda r: r["frames"][0].update(mainCommand=None),
            lambda r: r["frames"][0].update(offlineRequest=2),
            lambda r: r["frames"][0]["allocationFrame"].update(id=2),
            lambda r: r["frames"][0]["allocationFrame"].update(pendingSubmissions=1),
            lambda r: r["frames"][0].update(commandTimestampReadbackBytes=0),
            lambda r: r["gpuCommandTrace"]["records"][0]["contexts"][0].update(request=2),
        ]
        for index, mutate in enumerate(changes):
            with self.subTest(mutation=index):
                report = command_fixture("static")
                mutate(report)
                with self.assertRaisesRegex(ValueError, "Command trace"):
                    study.validate_case(report, "static", SOURCE, 3)

    def test_preparation_outside_root_timestamps_cannot_disappear_from_declared_cases(self):
        for name, stage in [("four-4k-yuv", "media_preparation"),
                            ("nested-matte", "nested_composition")]:
            with self.subTest(case=name):
                report = command_fixture(name)
                for record in report["gpuCommandTrace"]["records"]:
                    if record["stage"] == stage:
                        record["stage"] = "unclassified"
                with self.assertRaisesRegex(ValueError, "interval.*missing"):
                    study.validate_case(report, name, SOURCE, 3)
        report = command_fixture("static")
        report["frames"][0]["vectorUpload"]["encodedUploadBytesTotal"] = 0
        with self.assertRaisesRegex(ValueError, "vector preparation uploads missing"):
            study.validate_case(report, "static", SOURCE, 3)

    def test_unavailable_and_historical_evidence_remain_explicitly_unknown(self):
        old = study.validate_case(fixture(), "animated", SOURCE, 3)
        self.assertIsNone(old["commandEvidence"])
        report = command_fixture()
        report.update(commandTimestampSupported=False, gpuCommands=None, gpuCommandTrace=None)
        for frame in report["frames"]:
            frame.update(mainCommand=None, commandTimestampReadbackBytes=0)
        evidence = study.validate_case(report, "animated", SOURCE, 3)["commandEvidence"]
        self.assertFalse(evidence["available"])
        self.assertIsNone(evidence["nativeCommands"])
        report["frames"][0]["mainCommand"] = {"epoch": 7, "id": 1}
        with self.assertRaisesRegex(ValueError, "unavailable"):
            study.validate_case(report, "animated", SOURCE, 3)


if __name__ == "__main__":
    unittest.main()
