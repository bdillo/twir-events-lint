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

Merge events from the Meetup automation with the current draft events:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --new-events-file ~/scratch/14may
```

The command prints the merged event section, which can be copied into the draft.
