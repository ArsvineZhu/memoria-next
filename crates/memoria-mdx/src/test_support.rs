//! Test-only observability for downstream integration tests.
//!
//! The `test-support` Cargo feature makes this module available to another
//! crate's integration-test target; ordinary `cfg(test)` would only apply to
//! memoria-mdx's own unit-test target.

use std::cell::Cell;

thread_local! {
    static IR_COMPILE_CALLS: Cell<usize> = const { Cell::new(0) };
}

pub fn reset_ir_compile_calls() {
    IR_COMPILE_CALLS.with(|calls| calls.set(0));
}

#[must_use]
pub fn ir_compile_calls() -> usize {
    IR_COMPILE_CALLS.with(Cell::get)
}

pub(crate) fn record_ir_compile_call() {
    IR_COMPILE_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
}
