# Lint

Lint a TWIR draft:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md
```

Remove events that do not overlap the newsletter's event date range:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md --fix
```

Fixes are prepared in memory and then atomically written back to the draft. If every event in a region is removed, the empty region is removed as well.

# Merge

Preview a merge of externally collected events with the current draft:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --new-events-file ~/scratch/14may
```

Write the merged event listing back to the draft:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --new-events-file ~/scratch/14may \
  --in-place
```

Incoming events outside the newsletter date range are logged and omitted. The complete candidate document is linted before any changes are atomically written.
