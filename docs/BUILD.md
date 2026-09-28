# Isolated build and validation

Use an isolated development workspace with explicit resource limits; do not use production base paths, signing identities or service controls. Provide at least 16 GiB RAM, two build CPUs and 30 GiB free scratch space as planning allowances; actual needs depend on toolchain/cache. The retained initial publication preparation reused the release build. Earlier development separately built a normal node and Wasm and ran isolated signed upgrade/workflow checks. The final completion preparation additionally builds and qualifies an optimized release node; build success alone does not authorize production adoption.

Ubuntu build prerequisites: a C/C++ toolchain, clang/LLVM 18/libclang, cmake, pkg-config, OpenSSL headers, protobuf compiler, git and Python3. Install Rust 1.87.0 using the official Rust installer; the checked-in rust-toolchain.toml pins Rust, rustfmt/clippy and wasm32 target. Add rust-src for the build-std path.

```bash
rustup component add rust-src --toolchain 1.87.0
rustup target add wasm32-unknown-unknown --toolchain 1.87.0
python3 scripts/check-publication.py
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CC=clang CXX=clang++ AR=ar LIBCLANG_PATH=/usr/lib/llvm-18/lib
export PROTOC=/usr/bin/protoc WASM_BUILD_STD=1
export WASM_BUILD_WORKSPACE_HINT="$PWD"
export CARGO_PROFILE_RELEASE_OPT_LEVEL=2 CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_LTO=false
cargo +1.87.0 build --locked --release -p era-node --bin era-node --jobs 2
```

The retained approved build used the same settings with a verified offline dependency cache, Rust compiler pinned explicitly, 2 CPUs, MemoryHigh14GiB/Max16GiB, zero swap and 512 tasks. A fresh online dependency fetch must honor Cargo.lock; it is not evidence of bit-identical reproduction. Dependencies are not all vendored. Do not remove `--locked` or silently update them.

For future isolated changes, run focused tests then broaden where justified:

```bash
cargo +1.87.0 test --locked -p era-runtime --lib --jobs 2
cargo +1.87.0 test --locked -p era-runtime --test fresh_genesis --test principal_first --jobs 2
cargo +1.87.0 test --locked -p era-runtime --features try-runtime --test fresh_genesis --test principal_first --jobs 2
cargo +1.87.0 test --locked -p era-security-backport-smoke --jobs 2
cargo +1.87.0 clippy --locked -p era-node -p era-runtime --all-targets --jobs 2
cargo +1.87.0 fmt --all -- --check
```

Formatting/lint baselines and a clean hosted CI build are not certified by the carried runtime tests. CI provides separate manual bounded jobs; it has no deployment credentials or mainnet transaction step. Do not run fresh-spec/fresh-check using production base paths or real keys. Disposable synthetic tests must use unique directories and localhost-only network configuration.


For the new focused suites, use `cargo test --locked -p era-runtime --lib v14_completion_tests`, `cargo test --locked -p era-runtime --lib v14_ai`, `cargo test --locked -p pallet-ai-predictions --lib`, `python3 -m unittest discover -s tools/ai-evaluation -v`, and `node --test clients/native-sdk/tests/*.test.mjs`. The signed harness under `tools/v14-evaluation-signed-smoke` hard-binds loopback21944 and rejects the production genesis; it uses public development accounts only. Its synthetic commissioning is never an instruction to fund production. Preserve failed-attempt logs and reconcile pending transactions before retrying.

Development benchmark builds use the `runtime-benchmarks` feature. The pinned unrelated `frame-storage-access-test-runtime` auxiliary build may require `SKIP_FRAME_STORAGE_ACCESS_TEST_RUNTIME_WASM_BUILD=1` with this offline workspace; ERA runtime Wasm must remain enabled. A benchmark-enabled runtime blob is not the normal service blob. Verify the exact normal Wasm and its SDK-format compressed equivalent, executable size limits and finalized state preservation before any release proposal.


The20 September completion candidate uses the same Rust1.87/offline lock/cache and two-job limit, but the actual final optimized command uses Cargo's default release settings, with `CARGO_PROFILE_RELEASE_OPT_LEVEL`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS` and `CARGO_PROFILE_RELEASE_LTO` unset. Its generated Wasm workspace uses release thin LTO and panic abort. Do not confuse these artifact hashes with the historical opt-level2 command above. Command: `SKIP_FRAME_STORAGE_ACCESS_TEST_RUNTIME_WASM_BUILD=1 WASM_BUILD_WORKSPACE_HINT="$PWD" cargo +1.87.0 build --manifest-path Cargo.toml -p era-node --release --locked --jobs 2`. ERA Wasm is enabled. Benchmark qualification adds `--features runtime-benchmarks` and preserves the normal binary first; benchmark binaries must never replace the proposed node.

The release-embedded compressed Wasm differs from the earlier normal-development artifact. Its dedicated gates therefore include a fresh signed spec14→15 upgrade,1,302-block Wasm/native execution with signed reward claim, complete nonfinancial service scenario, signed application and allocator workflows, copied-production-state initialization and optimized weight-envelope comparisons. Metadata remains identical, so SDK/extension regressions are carried rather than relabelled as rerun.
