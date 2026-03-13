use architecture_knowledge::{ArchitecturePatternKind, KnowledgeAnalyzer};
use architecture_reasoner::ReverseArchitectureReasoner;
use code_ir::CodeIr;
use design_domain::DesignUnit;

#[test]
fn test15_pattern_detection() {
    let mut gateway = DesignUnit::new(1, "ApiGateway");
    gateway.dependencies.push(design_domain::DesignUnitId(2));
    let mut service = DesignUnit::new(2, "BillingService");
    service.dependencies.push(design_domain::DesignUnitId(3));
    let repository = DesignUnit::new(3, "BillingRepository");
    let graph = ReverseArchitectureReasoner
        .infer_from_code_ir(&CodeIr::from_design_units(&[gateway, service, repository]));

    let detection = KnowledgeAnalyzer::default().detect(&graph);

    assert!(
        detection
            .matched_patterns
            .iter()
            .any(|pattern| pattern.kind == ArchitecturePatternKind::Layered)
    );
    assert!(
        detection
            .matched_patterns
            .iter()
            .any(|pattern| pattern.kind == ArchitecturePatternKind::Microservice)
    );
}
