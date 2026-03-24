use crate::service::dto::AnalyzeResultDTO;

pub fn render(result: AnalyzeResultDTO) -> String {
    result.summary
}
