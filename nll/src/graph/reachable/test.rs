use graph::test::TestGraph;
use super::reachable;

#[test]
fn test1() {
    // 0 -> 1 -> 2 -> 3
    //      ^    v
    //      6 <- 4 -> 5
    let graph = TestGraph::new(0, &[
        (0, 1),
        (1, 2),
        (2, 3),
        (2, 4),
        (4, 5),
        (4, 6),
        (6, 1),
    ]);
    let reachable = reachable(&graph);
    assert!((0..6).all(|i| reachable.can_reach(0, i)));
    assert!((1..6).all(|i| reachable.can_reach(1, i)));
    assert!((1..6).all(|i| reachable.can_reach(2, i)));
    assert!((1..6).all(|i| reachable.can_reach(4, i)));
    assert!((1..6).all(|i| reachable.can_reach(6, i)));
    assert!(reachable.can_reach(3, 3));
    assert!(!reachable.can_reach(3, 5));
    assert!(!reachable.can_reach(5, 3));
}
