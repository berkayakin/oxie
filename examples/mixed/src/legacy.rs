//! Existing hand-written Rust code, used by the Oxie module.

pub const TAX_PERCENT: u64 = 20;

/// Formats cents as dollars.
pub fn money(cents: u64) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}
