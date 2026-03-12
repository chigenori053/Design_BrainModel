use architecture_domain::ArchitectureState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyCategory {
    Safety,
    Legal,
    Security,
    Subscription,
    Capability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicySeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyEnforcement {
    Allow,
    Warn,
    Restrict,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRule {
    pub policy_id: &'static str,
    pub category: PolicyCategory,
    pub severity: PolicySeverity,
    pub enforcement: PolicyEnforcement,
    pub description: &'static str,
    pub patterns: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubscriptionStatus {
    Active,
    Expired,
    Suspended,
    Trial,
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Active
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlanTier {
    Basic,
    Pro,
    Enterprise,
}

impl Default for PlanTier {
    fn default() -> Self {
        Self::Enterprise
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FeatureAccess {
    ArchitectureSearch,
    CodeGeneration,
    WebSearch,
    KnowledgeImport,
}

impl Default for FeatureAccess {
    fn default() -> Self {
        Self::ArchitectureSearch
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaymentStatus {
    Current,
    Delinquent,
}

impl Default for PaymentStatus {
    fn default() -> Self {
        Self::Current
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuditDecision {
    Allow,
    Warn,
    Restrict,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditContext {
    pub user_id: String,
    pub plan_tier: PlanTier,
    pub subscription_status: SubscriptionStatus,
    pub payment_status: PaymentStatus,
}

impl Default for AuditContext {
    fn default() -> Self {
        Self {
            user_id: "system".to_string(),
            plan_tier: PlanTier::Enterprise,
            subscription_status: SubscriptionStatus::Active,
            payment_status: PaymentStatus::Current,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityLimits {
    pub max_search_depth: u32,
    pub beam_width: u32,
    pub knowledge_budget: u32,
}

impl CapabilityLimits {
    pub fn for_plan(plan: PlanTier) -> Self {
        match plan {
            PlanTier::Basic => Self {
                max_search_depth: 16,
                beam_width: 8,
                knowledge_budget: 500,
            },
            PlanTier::Pro => Self {
                max_search_depth: 64,
                beam_width: 16,
                knowledge_budget: 2000,
            },
            PlanTier::Enterprise => Self {
                max_search_depth: 128,
                beam_width: 32,
                knowledge_budget: 5000,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditTelemetryEvent {
    pub name: &'static str,
    pub decision: AuditDecision,
    pub policy_id: Option<&'static str>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditTelemetry {
    pub events: Vec<AuditTelemetryEvent>,
}

impl AuditTelemetry {
    pub fn push(
        &mut self,
        name: &'static str,
        decision: AuditDecision,
        policy_id: Option<&'static str>,
        detail: impl Into<String>,
    ) {
        self.events.push(AuditTelemetryEvent {
            name,
            decision,
            policy_id,
            detail: detail.into(),
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyRegistry {
    rules: Vec<PolicyRule>,
}

impl PolicyRegistry {
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self { rules }
    }

    pub fn rules(&self) -> &[PolicyRule] {
        &self.rules
    }
}

impl Default for PolicyRegistry {
    fn default() -> Self {
        Self::new(vec![
            PolicyRule {
                policy_id: "safety.malware_development",
                category: PolicyCategory::Safety,
                severity: PolicySeverity::Critical,
                enforcement: PolicyEnforcement::Block,
                description: "Blocks malware development requests.",
                patterns: &["malware", "ransomware", "keylogger", "trojan"],
            },
            PolicyRule {
                policy_id: "security.hacking_tools",
                category: PolicyCategory::Security,
                severity: PolicySeverity::Critical,
                enforcement: PolicyEnforcement::Block,
                description: "Blocks hacking tool requests.",
                patterns: &["exploit", "payload", "credential stuffing", "bruteforce"],
            },
            PolicyRule {
                policy_id: "security.unauthorized_access",
                category: PolicyCategory::Security,
                severity: PolicySeverity::Critical,
                enforcement: PolicyEnforcement::Block,
                description: "Blocks unauthorized access requests.",
                patterns: &[
                    "unauthorized access",
                    "privilege escalation",
                    "bypass authentication",
                ],
            },
            PolicyRule {
                policy_id: "legal.copyright_circumvention",
                category: PolicyCategory::Legal,
                severity: PolicySeverity::High,
                enforcement: PolicyEnforcement::Block,
                description: "Blocks copyright circumvention requests.",
                patterns: &["drm bypass", "copyright circumvention", "license crack"],
            },
            PolicyRule {
                policy_id: "security.data_exfiltration",
                category: PolicyCategory::Security,
                severity: PolicySeverity::Critical,
                enforcement: PolicyEnforcement::Block,
                description: "Blocks data exfiltration requests.",
                patterns: &[
                    "data exfiltration",
                    "steal credentials",
                    "exfiltrate",
                    "dump secrets",
                ],
            },
        ])
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntentAuditor;

impl IntentAuditor {
    pub fn audit(
        &self,
        request: &str,
        registry: &PolicyRegistry,
        telemetry: &mut AuditTelemetry,
    ) -> AuditDecision {
        let normalized = request.to_ascii_lowercase();
        for rule in registry.rules() {
            if rule
                .patterns
                .iter()
                .any(|pattern| normalized.contains(&pattern.to_ascii_lowercase()))
            {
                let decision = enforcement_to_decision(rule.enforcement);
                telemetry.push(
                    "AuditPolicyViolation",
                    decision,
                    Some(rule.policy_id),
                    format!("intent matched blocked pattern for {}", rule.description),
                );
                return decision;
            }
            telemetry.push(
                "AuditPolicyChecked",
                AuditDecision::Allow,
                Some(rule.policy_id),
                format!("intent checked against {}", rule.description),
            );
        }
        AuditDecision::Allow
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureAuditor;

impl ArchitectureAuditor {
    pub fn audit(
        &self,
        architecture_state: &ArchitectureState,
        registry: &PolicyRegistry,
        telemetry: &mut AuditTelemetry,
    ) -> AuditDecision {
        let component_text = architecture_state
            .components
            .iter()
            .flat_map(|component| {
                [
                    format!("{:?}", component.role),
                    component
                        .inputs
                        .iter()
                        .map(|input| input.name.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                    component
                        .outputs
                        .iter()
                        .map(|output| output.name.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                ]
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        for rule in registry.rules() {
            if rule
                .patterns
                .iter()
                .any(|pattern| component_text.contains(&pattern.to_ascii_lowercase()))
            {
                let decision = enforcement_to_decision(rule.enforcement);
                telemetry.push(
                    "AuditPolicyViolation",
                    decision,
                    Some(rule.policy_id),
                    "architecture node violated policy",
                );
                return decision;
            }
        }

        AuditDecision::Allow
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessController;

impl AccessController {
    pub fn check(
        &self,
        context: &AuditContext,
        feature: FeatureAccess,
        telemetry: &mut AuditTelemetry,
    ) -> AuditDecision {
        let allowed = match (context.plan_tier, feature) {
            (PlanTier::Basic, FeatureAccess::ArchitectureSearch) => true,
            (PlanTier::Basic, FeatureAccess::CodeGeneration) => true,
            (PlanTier::Basic, FeatureAccess::WebSearch) => false,
            (PlanTier::Basic, FeatureAccess::KnowledgeImport) => false,
            (PlanTier::Pro, FeatureAccess::KnowledgeImport) => true,
            (PlanTier::Pro, _) => true,
            (PlanTier::Enterprise, _) => true,
        };

        if allowed {
            telemetry.push(
                "AuditPolicyChecked",
                AuditDecision::Allow,
                Some("capability.feature_access"),
                format!("feature {:?} allowed", feature),
            );
            AuditDecision::Allow
        } else {
            telemetry.push(
                "AuditCapabilityRestricted",
                AuditDecision::Restrict,
                Some("capability.feature_access"),
                format!(
                    "feature {:?} restricted for {:?}",
                    feature, context.plan_tier
                ),
            );
            AuditDecision::Restrict
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubscriptionController;

impl SubscriptionController {
    pub fn check(&self, context: &AuditContext, telemetry: &mut AuditTelemetry) -> AuditDecision {
        match (context.subscription_status, context.payment_status) {
            (SubscriptionStatus::Expired, _) | (_, PaymentStatus::Delinquent) => {
                telemetry.push(
                    "AuditSubscriptionBlocked",
                    AuditDecision::Block,
                    Some("subscription.expired"),
                    "subscription inactive",
                );
                AuditDecision::Block
            }
            (SubscriptionStatus::Suspended, _) => {
                telemetry.push(
                    "AuditSubscriptionBlocked",
                    AuditDecision::Restrict,
                    Some("subscription.suspended"),
                    "subscription suspended",
                );
                AuditDecision::Restrict
            }
            (SubscriptionStatus::Trial, _) => {
                telemetry.push(
                    "AuditPolicyChecked",
                    AuditDecision::Restrict,
                    Some("subscription.trial"),
                    "trial capability limits applied",
                );
                AuditDecision::Restrict
            }
            (SubscriptionStatus::Active, PaymentStatus::Current) => {
                telemetry.push(
                    "AuditPolicyChecked",
                    AuditDecision::Allow,
                    Some("subscription.active"),
                    "subscription active",
                );
                AuditDecision::Allow
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyEngine;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditResult {
    pub decision: AuditDecision,
    pub capability_limits: CapabilityLimits,
    pub telemetry: AuditTelemetry,
}

impl PolicyEngine {
    pub fn evaluate_request(
        &self,
        context: &AuditContext,
        feature: FeatureAccess,
        request: &str,
        registry: &PolicyRegistry,
        intent_auditor: &IntentAuditor,
        access_controller: &AccessController,
        subscription_controller: &SubscriptionController,
    ) -> AuditResult {
        let mut telemetry = AuditTelemetry::default();
        let subscription_decision = subscription_controller.check(context, &mut telemetry);
        if subscription_decision == AuditDecision::Block {
            return AuditResult {
                decision: AuditDecision::Block,
                capability_limits: CapabilityLimits::for_plan(context.plan_tier),
                telemetry,
            };
        }

        let access_decision = access_controller.check(context, feature, &mut telemetry);
        let intent_decision = intent_auditor.audit(request, registry, &mut telemetry);
        let capability_limits = capability_limits_for_context(context);
        let decision = max_decision(
            subscription_decision,
            max_decision(access_decision, intent_decision),
        );

        AuditResult {
            decision,
            capability_limits,
            telemetry,
        }
    }

    pub fn evaluate_architecture(
        &self,
        architecture_state: &ArchitectureState,
        registry: &PolicyRegistry,
        architecture_auditor: &ArchitectureAuditor,
    ) -> AuditResult {
        let mut telemetry = AuditTelemetry::default();
        let decision = architecture_auditor.audit(architecture_state, registry, &mut telemetry);
        AuditResult {
            decision,
            capability_limits: CapabilityLimits::for_plan(PlanTier::Enterprise),
            telemetry,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditCore {
    pub policy_registry: PolicyRegistry,
    pub policy_engine: PolicyEngine,
    pub intent_auditor: IntentAuditor,
    pub architecture_auditor: ArchitectureAuditor,
    pub access_controller: AccessController,
    pub subscription_controller: SubscriptionController,
}

impl AuditCore {
    pub fn audit_request(
        &self,
        context: &AuditContext,
        feature: FeatureAccess,
        request: &str,
    ) -> AuditResult {
        self.policy_engine.evaluate_request(
            context,
            feature,
            request,
            &self.policy_registry,
            &self.intent_auditor,
            &self.access_controller,
            &self.subscription_controller,
        )
    }

    pub fn audit_architecture(&self, architecture_state: &ArchitectureState) -> AuditResult {
        self.policy_engine.evaluate_architecture(
            architecture_state,
            &self.policy_registry,
            &self.architecture_auditor,
        )
    }
}

fn capability_limits_for_context(context: &AuditContext) -> CapabilityLimits {
    let mut limits = CapabilityLimits::for_plan(context.plan_tier);
    match context.subscription_status {
        SubscriptionStatus::Trial => {
            limits.max_search_depth = limits.max_search_depth.min(16);
            limits.beam_width = limits.beam_width.min(8);
            limits.knowledge_budget = limits.knowledge_budget.min(500);
        }
        SubscriptionStatus::Suspended => {
            limits.max_search_depth = limits.max_search_depth.min(8);
            limits.beam_width = limits.beam_width.min(4);
            limits.knowledge_budget = limits.knowledge_budget.min(250);
        }
        SubscriptionStatus::Expired | SubscriptionStatus::Active => {}
    }
    limits
}

fn enforcement_to_decision(enforcement: PolicyEnforcement) -> AuditDecision {
    match enforcement {
        PolicyEnforcement::Allow => AuditDecision::Allow,
        PolicyEnforcement::Warn => AuditDecision::Warn,
        PolicyEnforcement::Restrict => AuditDecision::Restrict,
        PolicyEnforcement::Block => AuditDecision::Block,
    }
}

fn max_decision(lhs: AuditDecision, rhs: AuditDecision) -> AuditDecision {
    use AuditDecision::{Allow, Block, Restrict, Warn};
    match (lhs, rhs) {
        (Block, _) | (_, Block) => Block,
        (Restrict, _) | (_, Restrict) => Restrict,
        (Warn, _) | (_, Warn) => Warn,
        _ => Allow,
    }
}

#[cfg(test)]
mod tests {
    use architecture_domain::ArchitectureState;
    use design_domain::{Architecture, DesignUnit, Layer};

    use super::*;

    #[test]
    fn malicious_intent_is_blocked() {
        let audit = AuditCore::default();
        let result = audit.audit_request(
            &AuditContext::default(),
            FeatureAccess::ArchitectureSearch,
            "design a stealth keylogger malware",
        );

        assert_eq!(result.decision, AuditDecision::Block);
        assert!(
            result
                .telemetry
                .events
                .iter()
                .any(|event| event.name == "AuditPolicyViolation")
        );
    }

    #[test]
    fn trial_subscription_is_restricted() {
        let audit = AuditCore::default();
        let result = audit.audit_request(
            &AuditContext {
                user_id: "trial-user".to_string(),
                plan_tier: PlanTier::Pro,
                subscription_status: SubscriptionStatus::Trial,
                payment_status: PaymentStatus::Current,
            },
            FeatureAccess::ArchitectureSearch,
            "build a normal web api",
        );

        assert_eq!(result.decision, AuditDecision::Restrict);
        assert_eq!(result.capability_limits.max_search_depth, 16);
        assert_eq!(result.capability_limits.beam_width, 8);
    }

    #[test]
    fn suspicious_architecture_is_blocked() {
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::with_layer(
            9,
            "KeyloggerService",
            Layer::Service,
        ));
        let architecture_state = ArchitectureState::from_architecture(&architecture, Vec::new());

        let result = AuditCore::default().audit_architecture(&architecture_state);

        assert_eq!(result.decision, AuditDecision::Block);
    }
}
