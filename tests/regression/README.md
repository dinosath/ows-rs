# Regression

Every discovered bug must get a permanent regression test. Regression tests live
in the crate that owns the bug (usually `crates/ows/tests/errors.rs` or
`crates/ows-expressions/tests/`). This directory may hold scenarios that
span multiple crates.
