use crate::domain::state::UnifiedDesignState;

const FNV_OFFSET_BASIS_64: u64 = 0xcbf29ce484222325;
const FNV_PRIME_64: u64 = 0x100000001b3;

pub fn compute_hash(uds: &UnifiedDesignState) -> u64 {
    let canonical = canonical_serialize(uds);
    fnv1a_64(canonical.as_bytes())
}

pub fn canonical_serialize(uds: &UnifiedDesignState) -> String {
    let mut out = String::new();

    for (key, value) in &uds.nodes {
        out.push_str("N|");
        out.push_str(&normalize_whitespace(key));
        out.push('|');
        out.push_str(&normalize_whitespace(value));
        out.push('\n');
    }

    for (key, deps) in &uds.dependencies {
        let mut normalized_deps = deps
            .iter()
            .map(|d| normalize_whitespace(d))
            .collect::<Vec<_>>();
        normalized_deps.sort();
        normalized_deps.dedup();

        out.push_str("D|");
        out.push_str(&normalize_whitespace(key));
        out.push('|');
        out.push_str(&normalized_deps.join(","));
        out.push('\n');
    }

    for (key, state) in &uds.node_id_states {
        out.push_str("S|");
        out.push_str(&normalize_whitespace(key));
        out.push('|');
        out.push_str(match state {
            crate::domain::state::NodeIdState::Temporary => "temporary",
            crate::domain::state::NodeIdState::PendingPromotion => "pending_promotion",
            crate::domain::state::NodeIdState::Global => "global",
        });
        out.push('\n');
    }

    for (key, origin) in &uds.node_origins {
        out.push_str("O|");
        out.push_str(&normalize_whitespace(key));
        out.push('|');
        out.push_str(&normalize_whitespace(origin));
        out.push('\n');
    }

    out
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS_64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}
