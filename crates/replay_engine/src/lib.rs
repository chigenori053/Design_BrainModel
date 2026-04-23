pub mod capture;
pub mod classify;
pub mod diff;
pub mod replay;
pub mod trace;
mod tests;

pub use capture::capture;
pub use classify::FailureClass;
pub use diff::{diff, DiffReport, LayerDiff, MatchStatus};
pub use replay::replay;
pub use trace::FullTrace;
