use super::support::assert_deterministic;

#[test]
fn search_tie_break_is_canonicalized() {
    assert_deterministic("break-search-tie");
}
