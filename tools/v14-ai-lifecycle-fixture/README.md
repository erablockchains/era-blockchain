# AI capability lifecycle fixture

This fixture preserves the existing AI prediction/token interfaces. `generate.py` runs a deterministic moving-average rule off-chain using Python's standard library and writes canonical model, input/output, observation and dispute-witness JSON plus SHA256 commitments. It performs no network I/O, paid computation, training, signing or production operation. Its confidence value is synthetic and uncalibrated.

`runtime/src/v14_ai_lifecycle_tests.rs` reads those bytes, verifies their commitments, encodes and decodes actual `RuntimeCall::AiPredictions` calls, and dispatches them against `Runtime` in `TestExternalities` with synthetic origins and balances. The six cases cover correct, ordinary incorrect, good-faith rejected dispute, inconsistent claimed model execution, fabricated dispute evidence and a market with no winning stake. Root's fraud finding is simulated after inspecting controlled fixture evidence; the chain does not prove execution or evidence correctness.

The test exercises registration/approval/delegation, unauthorized submission rejection, tokenization and delegated ownership transfer, passive and outcome-side funding, validation, dispute/adjudication, claims, duplicate protection and retirement. It exports per-stage financial snapshots when `ERA_V14_AI_LIFECYCLE_OUTPUT` names a local output file. An additional five boundary regressions protect admission and cancellation; three characterization tests document unresolved provenance/timing/recovery limitations; one preservation test covers existing storage-version3 records and refund rights.

Run from the isolated source directory:

```sh
python3 tools/v14-ai-lifecycle-fixture/generate.py
ERA_V14_AI_LIFECYCLE_OUTPUT=/tmp/era-v14-ai-lifecycle-local.json SKIP_WASM_BUILD=1 ../cargo-local.sh test --manifest-path Cargo.toml -p era-runtime --lib v14_ai_lifecycle_tests --locked -- --nocapture
```

Use the controlled evidence output path when retaining results. Tests use 18-decimal synthetic ETKN but the fixture values are not production parameters. Native dispatch bypasses signature/nonce verification, transaction fees, block admission and finality. Weight::MAX development gates therefore do not prevent these tests; they still prevent claiming a deployable release. No trained model, production inference runner, live wallet, RPC workflow or commercial service has been validated.

The retained historical `source/tools/phase6b-signed-smoke` outside this snapshot is a separate spec9-only localhost transaction tool with public development signers and fixed hashes. It is not an inference runner and is not executed by this fixture. Existing public candidates, production deployment and the completed sampled observation remain unchanged.
