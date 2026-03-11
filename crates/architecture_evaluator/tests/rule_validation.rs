use architecture_reasoner::ReverseArchitectureReasoner;
use architecture_rules::{ArchitectureRule, RuleValidator};
use code_ir::CodeIr;
use design_domain::DesignUnit;

#[test]
fn test14_rule_enforcement() {
    let mut repository = DesignUnit::new(1, "UserRepository");
    repository.dependencies.push(design_domain::DesignUnitId(2));
    let mut service = DesignUnit::new(2, "UserService");
    service.dependencies.push(design_domain::DesignUnitId(1));
    let graph = ReverseArchitectureReasoner
        .infer_from_code_ir(&CodeIr::from_design_units(&[repository, service]));

    let violations = RuleValidator.validate(&graph);

    assert!(violations
        .iter()
        .any(|violation| matches!(violation.rule, ArchitectureRule::NoDependencyCycle)));
    assert!(violations
        .iter()
        .any(|violation| matches!(violation.rule, ArchitectureRule::LayerViolation)));
}
