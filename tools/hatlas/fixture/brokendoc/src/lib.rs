//! A dependency that compiles and fails only under rustdoc, so a run can be shown to survive it.

#[cfg(doc)]
compile_error!("hatlas fixture: fails only under rustdoc");

/// A value for the fixture to call.
pub fn answer() -> u32 {
    42
}
