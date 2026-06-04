use core_types::{CanonicalReuseEvent, CanonicalReuseResolver, FollowupResolution, ReuseScope};

use crate::{RuntimeEvent, RuntimeEventBus, intent_refiner::ChatContext};

pub struct FollowupResolver {
    resolver: CanonicalReuseResolver,
}

impl FollowupResolver {
    pub fn new(resolver: CanonicalReuseResolver) -> Self {
        Self { resolver }
    }

    pub fn resolve(
        &mut self,
        input: &str,
        context: &ChatContext,
        scope: ReuseScope,
        events: &mut RuntimeEventBus,
    ) -> FollowupResolution {
        let resolution = core_types::resolve_followup_context_with_events(
            &mut self.resolver,
            input,
            &context.history,
            scope,
        );
        for event in &resolution.events {
            events.publish(runtime_event_for_canonical_event(event));
        }
        if resolution.decision == core_types::ReuseDecision::Reuse
            && !resolution
                .events
                .contains(&CanonicalReuseEvent::ContinuationResolved)
        {
            events.publish(RuntimeEvent::ContinuationResolved);
        }
        let resolution = FollowupResolution {
            reused: resolution.decision == core_types::ReuseDecision::Reuse,
            confidence: match resolution.match_kind {
                core_types::CanonicalMatchKind::ExactSource => 1.0,
                core_types::CanonicalMatchKind::Semantic => 0.86,
                core_types::CanonicalMatchKind::TrajectoryContinuation => 0.78,
                core_types::CanonicalMatchKind::Novel => 0.0,
            },
            canonical_ref: resolution.canonical_ref,
        };
        resolution
    }

    pub fn resolver(&self) -> &CanonicalReuseResolver {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut CanonicalReuseResolver {
        &mut self.resolver
    }
}

fn runtime_event_for_canonical_event(event: &CanonicalReuseEvent) -> RuntimeEvent {
    match event {
        CanonicalReuseEvent::ReuseObserved => RuntimeEvent::ReuseObserved,
        CanonicalReuseEvent::ReuseResolved => RuntimeEvent::ReuseResolved,
        CanonicalReuseEvent::CanonicalCreated => RuntimeEvent::CanonicalCreated,
        CanonicalReuseEvent::AliasRegistered => RuntimeEvent::AliasRegistered,
        CanonicalReuseEvent::DuplicateMerged => RuntimeEvent::DuplicateMerged,
        CanonicalReuseEvent::ContinuationResolved => RuntimeEvent::ContinuationResolved,
        CanonicalReuseEvent::ExactDuplicateMerged => RuntimeEvent::ExactDuplicateMerged,
        CanonicalReuseEvent::SemanticAliasRegistered => RuntimeEvent::SemanticAliasRegistered,
        CanonicalReuseEvent::CanonicalMemorySelected => RuntimeEvent::CanonicalMemorySelected,
        CanonicalReuseEvent::CanonicalMemoryReused => RuntimeEvent::CanonicalMemoryReused,
        CanonicalReuseEvent::CanonicalClusterExpanded => RuntimeEvent::CanonicalClusterExpanded,
    }
}

#[cfg(test)]
mod tests {
    use core_types::{CanonicalReuseResolver, ReuseScope};

    use crate::{RuntimeEvent, RuntimeEventBus, canonical_followup::FollowupResolver};

    #[test]
    fn followup_resolver_emits_canonical_reuse_events() {
        let context = crate::ChatContext {
            history: vec!["previous canonical memory work".to_string()],
            last_slots: None,
        };
        let mut events = RuntimeEventBus::default();
        let mut resolver = FollowupResolver::new(CanonicalReuseResolver::new());

        let first = resolver.resolve(
            "continue canonical memory work",
            &context,
            ReuseScope::Global,
            &mut events,
        );
        let second = resolver.resolve(
            "continue canonical memory work",
            &context,
            ReuseScope::Global,
            &mut events,
        );
        let drained = events.drain();

        assert!(!first.reused);
        assert!(second.reused);
        assert!(drained.contains(&RuntimeEvent::CanonicalCreated));
        assert!(drained.contains(&RuntimeEvent::ContinuationResolved));
        assert!(drained.contains(&RuntimeEvent::ReuseResolved));
    }
}
