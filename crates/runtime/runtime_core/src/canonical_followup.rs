use core_types::{
    CanonicalReuseResolver, FollowupResolution, ReuseScope, resolve_followup_context,
};

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
        events.publish(RuntimeEvent::ReuseObserved);
        let resolution =
            resolve_followup_context(&mut self.resolver, input, &context.history, scope);
        if resolution.reused {
            events.publish(RuntimeEvent::AliasRegistered);
            events.publish(RuntimeEvent::ContinuationResolved);
        } else {
            events.publish(RuntimeEvent::CanonicalCreated);
        }
        events.publish(RuntimeEvent::ReuseResolved);
        resolution
    }

    pub fn resolver(&self) -> &CanonicalReuseResolver {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut CanonicalReuseResolver {
        &mut self.resolver
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
