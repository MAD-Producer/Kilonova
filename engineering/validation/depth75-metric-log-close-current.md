# Preserve metric export failures during log closure (#75)

The metric CLI's final file-handler closure can fail independently of export.
Its previous unconditional `finally: sink.close()` could replace the original
exception. The CLI now retains an active primary exception, writes the secondary
closure cause directly to stderr and lets the original failure propagate.
After successful export, a closure failure still fails the command; a successful
model calculation does not imply that its log was finalized successfully.
The existing phase/file Error record is persisted before closure where possible.
No diagnostic writes to stdout or retries the model.

Two subprocess controls cover these distinct paths. The failure control runs
the actual export admission against a missing reference checkout, preserving
its original git/CalledProcessError, default Info start and file Error record.
Only file closure is made to fail afterward. The other control substitutes the
successful export body to isolate CLI finalization; it does not claim model,
publication or sensor accuracy. It verifies that the closure error is observable
and produces a nonzero exit. Both require empty stdout. Existing assertions,
numerical thresholds and ignores are unchanged.

Extracting the CLI into `main(argv)` initially left its phase observer using
`global` instead of `nonlocal`. The first complete discovery failed the new
actual-failure control: stderr reported `admission` rather than the entered
`verify_reference_and_weights` phase. Original session15588/terminal`080b9d`
is retained as failure,159 tests with one failure and one existing skip.
Correcting that closure to `nonlocal` fixes the captured phase without changing
the assertion or candidate computation.

The subsequent complete command used the existing E-drive environment with the
CI-required PyYAML6.0.3, without installing or changing packages:

```text
python -B -m unittest discover -s tools -p 'test_ci_*.py' -v
```

It completed159 tests:158 passes and one original skip, no filtering.
Session95992 exited0, original terminal`568b01` consumed; tracked tools hashes
were unchanged throughout. The [archive](data/depth75-metric-log-close-v1.zip)
and [manifest](data/depth75-metric-log-close-v1.json) retain both original commands,
receipts and logs, plus exact final source. CRC and all member readbacks passed.
The original failed source is identified by its receipt hashes; the included
source snapshot belongs to the final run, not the earlier failed phase variant.

The numerical export/preprocessing and candidate itself are unchanged by this
logging increment; their earlier measurements retain their exact source scope.
This does not clear the original unexplained fixture/native failures, qualify
the rejected metric sensor result or replace new cross-platform CI.
