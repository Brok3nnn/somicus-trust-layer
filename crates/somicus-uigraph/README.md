# somicus-uigraph

Platform-neutral UI Graph types and algorithms: the screen as nodes in global physical pixels, with stable element
fingerprints, deltas, a token-budgeted digest and ranked target candidates. In this repository it is the input type of
the Privacy Shield (`redact_uigraph`, `redact_digest`, `ClassifiedText`) and shared geometry for the Policy Guard.

## What it does

- Represents interface elements (`UiNode`, `NodeId`, `Role`, states, actions, relations, selectors) and a screen
  (`UiGraph` with its app, window, display and focus), as read from the operating system's accessibility tree.
- Owns the coordinate spaces: global physical pixels, display-local logical pixels and the transforms between them.
- Computes stable element fingerprints (`xxh3_64` over role, normalized name and structure; never values), minimal
  deltas between graphs, the screen digest within a token budget, and the top-K target candidates plus the
  `none_visible` option.
- Carries each text's privacy class (`ClassifiedText`, `PrivacyClass`: public label, content value, personal, secret)
  for the Privacy Shield.
- Does not access operating-system APIs, redact, call models, log screen content or use the network.

## Dependencies

`serde`, `thiserror` and `twox-hash`; no other Somicus crate. It forbids `unsafe` code.

## Testing

`cargo test -p somicus-uigraph`. The tests cover candidate ranking, coordinates and deltas, the digest budget,
fingerprint properties, untrusted coordinates and the public crate boundary; the fingerprint and coordinate tests use
`proptest`.
