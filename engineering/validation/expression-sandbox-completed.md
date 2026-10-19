# Expression capability and project-isolation verification

## Scope and coordination

This batch addresses issue #135, section VI: unavailable expression capabilities and cross-project isolation. The baseline is main `58bf6742`. Local branch heads and open PR files were checked before starting. PR #344 owns render sampling, #348 diamond bounds, #349 U/UU filtering, #350 camera/video composition tests, #351 unit-test preferences and #352 model/import robustness. None modifies the expression runtime or transpiler. No panel code is in scope.

The baseline already creates a fresh evaluation context for every call, caches bytecode per thread, limits memory and execution, rejects asynchronous source functions/await and hides the global Date, Promise, SharedArrayBuffer and Atomics bindings. Those are existing mechanisms, not new fixes in this batch. Existing tests verify seek-order independence within one project; they do not explicitly exercise two projects sharing identical cached source and target IDs.

The full context in pinned rquickjs 0.14 and its bundled QuickJS also registers `performance`, `queueMicrotask` and source evaluation. Clearing global constructor names alone does not disable function-constructor compilation through prototypes. Dynamic import is promise-based but was not rejected by the source adapter's asynchronous-syntax guard.

## Implementation and verification

Evaluation now uses a custom context that omits the Eval, Date and Performance intrinsics. Thus removing the eval/Function bindings is backed by disabling source compilation in the context itself: ordinary, arrow, native, generator, reflected, bound and proxied constructor calls cannot re-enable it. Compilation still uses the existing full context only to produce bytecode for the fixed module wrapper; it does not execute the user function. Bytecode cache size/lifetime, fresh evaluation contexts, memory/stack limits, deadlines and seeded random behavior are unchanged.

Promise support remains internal because the existing precompiled-module evaluator returns a promise. Its global binding and queueMicrotask are hidden, along with the existing SharedArrayBuffer/Atomics bindings. Dynamic import is rejected by the source adapter before it can create asynchronous work, using the same asynchronous-source rejection as async functions/await.

The previous context's regular-expression, JSON, Proxy, collection, typed-array and WeakRef intrinsics remain available. The existing dependency's public Base64 initializer preserves atob/btoa, for which rquickjs 0.14 provides no intrinsic marker. This is a checked initialization call inside Context::with's runtime lock, with a live context, no escaping raw pointer/value, and the original JavaScript exception converted through the existing error helper on failure. No dependency or third-party implementation is added.

This is a functional restriction of previously reachable out-of-profile capabilities, not a logging-only patch. Ordinary expression evaluation and the existing Error::Expression target/frame/cause structure remain intact.

Seven new regressions exercise the real public project/command evaluation path: named forbidden globals and host APIs; direct/indirect eval and constructor/reflection routes; early dynamic-import rejection; a forbidden branch first reached at a later frame; ordinary JavaScript compatibility; two projects sharing source/object IDs with distinct samples and composition widths; and recovery after an exception or interrupted loop poisons its own globals/prototypes.

The compatibility controls cover ordinary functions, generators, arrays, JSON, Map, Set, typed arrays, regex literals/constructors and Base64 helpers. The isolation tests alternate projects on the same thread, keep identical source cached, assert independent results and unchanged original project snapshots, and then repeat after failed evaluations. Existing assertions are not changed.

The first six-test baseline run produced **3 passed, 3 failed**, exposing performance/eval/Function/queueMicrotask, accepting dynamic compilation and rejecting dynamic import only after creating async work. The bound-constructor fixture was then completed with invocation of its generated function, and a later-frame regression was added. Against the same unchanged production code, the final seven-test baseline produced **3 passed, 4 failed**: all 14 direct/indirect/constructor routes executed, and the deferred frame also compiled dynamically. The existing isolation, failure recovery and ordinary-JavaScript controls passed. Evidence: `artifacts/issue-135-sandbox-before.log` and `issue-135-sandbox-before-extended.log`.

With the runtime/transpiler change, the same final seven regressions and assertions pass: **7 passed, 0 failed, 0 ignored**, in 0.24 seconds. The deferred failure preserves target metadata, frame 30 and a JavaScript cause, leaves authored animation unchanged and permits the earlier valid frame to evaluate again. Evidence: `artifacts/issue-135-sandbox-after.log`.

Full `cargo test -p motion-core --locked -j 1 -- --test-threads=1` completed successfully: **246 passed, 0 failed, 1 existing ignored manual large-document performance measurement**, across 49 result summaries including doc tests. Evidence: `artifacts/issue-135-sandbox-core.log`.

`cargo test -p motion-render --locked -j 1 --test expressions -- --test-threads=1` also completed successfully: **2 passed, 0 failed, 0 ignored**, in 8.32 seconds. These existing tests use a real headless GPU renderer and assert geometry, separated axes, effect values, rendered pixels and frozen export-plan parity. Evidence: `artifacts/issue-135-sandbox-render.log`. Both commands exited zero. Cargo runs use portable MSVC, single-threaded test execution and the dedicated target `E:/Dev/aem/target/observability-expression-validation`.

Acceptance ran on Windows locally. Other platforms remain subject to PR CI. Panel style and provenance guards, formatting of the new Rust test and context helper, and whitespace checks pass. The panel diff is empty. No native UI screenshot is required for this backend-only restriction.

Before submission, newly opened PR #353 (icon generation tools) and #354 (host registry diagnostics and desktop project-failure tests) were also checked; neither changes the expression runtime, source adapter or this test file.

## Remaining scope

Issue #135 remains open. The event-routing, GPU execution policy and additional model/device-health coverage are separate items; PR #351 retains its own preferences-isolation evidence. This batch does not certify the whole issue or every JavaScript-engine behavior. No UI presentation, dependency, resource, project format, expression profile identifier, worker timeout or restart was changed.
