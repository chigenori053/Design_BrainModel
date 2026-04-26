pub mod change_set;
pub mod ir_diff;
pub mod matcher;

pub use change_set::ChangeSet;
pub use ir_diff::{IrChange, diff_programs, replay_changes};
