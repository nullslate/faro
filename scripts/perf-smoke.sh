#!/usr/bin/env sh
set -eu

printf '%s\n' '== large session perf =='
cargo test large_session -- --ignored --nocapture

printf '%s\n' '== render perf =='
cargo test render_perf -- --ignored --nocapture
