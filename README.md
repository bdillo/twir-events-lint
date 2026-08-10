# Lint

Lint a TWIR draft:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md
```

Remove events that do not overlap the newsletter's event date range and reorder events within each region:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md --fix
```

Fixes are prepared in memory and then atomically written back to the draft. Reordered event blocks retain their original text. If every event in a region is removed, the empty region is removed as well.

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

## Fetch from Meetup

Fetch events for configured Meetup groups and preview the merged listing:

```sh
export MEETUP_PRIVATE_KEY="$HOME/.ssh/meetup_signing_key.pem"
export MEETUP_AUTHORIZED_MEMBER_ID="..."
export MEETUP_CLIENT_KEY="..."

cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --meetup-groups groups/rust-meetups.json
```

Meetup group files are arrays of records with a `url` and an optional `event_format` of `virtual` or `hybrid`:

```json
[
  { "url": "https://www.meetup.com/bcnrust" },
  {
    "url": "https://www.meetup.com/join-srug",
    "event_format": "hybrid"
  }
]
```

Add `--in-place` to atomically update the draft. Meetup collection uses the date range declared in the draft. `--meetup-groups` can be combined with `--new-events-file`; manually supplied events take precedence when event URLs overlap.

If the OAuth client has multiple signing keys or requires explicit key selection, also set `MEETUP_SIGNING_KEY_ID`; it is otherwise optional for compatibility with existing Meetup clients.
