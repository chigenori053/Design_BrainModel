mod consistency;
mod constants;
mod decide;
mod explain;
mod multi;
mod recommend;
mod report;

pub use consistency::{ConsistencyReport, TradeoffDetail};
pub use decide::{DecisionError, DecisionReport, DecisionWeights};
pub use explain::{Explanation, ResonanceReport};
pub use multi::{MultiConceptInput, MultiExplanation, MultiMetrics};
pub use recommend::{ActionType, Recommendation, RecommendationInput, RecommendationReport};
pub use report::DesignReport;

#[derive(Default)]
pub struct Recomposer;
