use std::path::{Path, PathBuf};

use crate::error::MutationError;
use crate::executor;
use crate::model::{
    AnalyzeContext, AuditResult, Confirmation, MutationApplyRecord, MutationAudit, MutationPlan,
    MutationPreview, MutationValidation,
};
use crate::preview;
use crate::store::MutationAuditStore;
use crate::validator::{MutationValidator, RuntimeValidator};

pub struct MutationEngine<'a> {
    workspace: PathBuf,
    actor: String,
    runtime: &'a dyn RuntimeValidator,
    store: MutationAuditStore,
}

impl<'a> MutationEngine<'a> {
    pub fn new(
        workspace: impl AsRef<Path>,
        actor: impl Into<String>,
        runtime: &'a dyn RuntimeValidator,
    ) -> Self {
        Self {
            workspace: workspace.as_ref().to_path_buf(),
            actor: actor.into(),
            runtime,
            store: MutationAuditStore::new(&workspace),
        }
    }

    pub fn plan(&self, plan: MutationPlan) -> Result<PlannedMutation<'_>, MutationError> {
        self.store.persist_plan(&plan)?;
        Ok(PlannedMutation { engine: self, plan })
    }
}

pub struct PlannedMutation<'a> {
    engine: &'a MutationEngine<'a>,
    plan: MutationPlan,
}

impl<'a> PlannedMutation<'a> {
    pub fn validate(
        self,
        analyze: &AnalyzeContext,
    ) -> Result<ValidatedMutation<'a>, MutationError> {
        let validation = MutationValidator::new(self.engine.runtime).validate(
            &self.engine.workspace,
            &self.plan,
            analyze,
        );
        if !validation.passed() {
            self.engine.store.persist_audit(&MutationAudit {
                mutation_id: self.plan.id.clone(),
                who: self.engine.actor.clone(),
                when: crate::executor::timestamp(),
                why: self.plan.reason.clone(),
                what: validation
                    .violations
                    .iter()
                    .map(|violation| violation.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
                result: AuditResult::Rejected,
            })?;
            return Err(MutationError::Rejected(
                validation
                    .violations
                    .iter()
                    .map(|violation| violation.message.clone())
                    .collect(),
            ));
        }
        Ok(ValidatedMutation {
            engine: self.engine,
            plan: self.plan,
            validation,
        })
    }
}

pub struct ValidatedMutation<'a> {
    engine: &'a MutationEngine<'a>,
    plan: MutationPlan,
    validation: MutationValidation,
}

impl<'a> ValidatedMutation<'a> {
    pub fn validation(&self) -> &MutationValidation {
        &self.validation
    }

    pub fn preview(self) -> Result<PreviewedMutation<'a>, MutationError> {
        let preview = preview::preview(&self.engine.workspace, &self.plan)?;
        self.engine.store.persist_preview(&preview)?;
        Ok(PreviewedMutation {
            engine: self.engine,
            plan: self.plan,
            validation: self.validation,
            preview,
        })
    }
}

pub struct PreviewedMutation<'a> {
    engine: &'a MutationEngine<'a>,
    plan: MutationPlan,
    validation: MutationValidation,
    preview: MutationPreview,
}

impl<'a> PreviewedMutation<'a> {
    pub fn preview_data(&self) -> &MutationPreview {
        &self.preview
    }

    pub fn apply(
        self,
        confirmation: Option<&Confirmation>,
    ) -> Result<MutationApplyRecord, MutationError> {
        executor::apply(executor::ApplyContext {
            workspace: &self.engine.workspace,
            actor: &self.engine.actor,
            plan: &self.plan,
            validation: &self.validation,
            preview: &self.preview,
            confirmation,
            runtime: self.engine.runtime,
            store: &self.engine.store,
        })
    }
}
