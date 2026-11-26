//! Integration smoke tests for the public crate boundary.

use somicus_uigraph as _;

#[test]
fn crate_links_as_a_workspace_member() {
    assert_eq!(env!("CARGO_PKG_NAME"), "somicus-uigraph");
}
