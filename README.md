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

## Fetch from event sources

Fetch Meetup and Luma events together and preview the merged listing:

```sh
export MEETUP_PRIVATE_KEY="$HOME/.ssh/meetup_signing_key.pem"
export MEETUP_AUTHORIZED_MEMBER_ID="..."
export MEETUP_CLIENT_KEY="..."

cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --event-sources groups/rust-event-sources.json
```

The source file contains separate Meetup and Luma arrays. Either array may be empty:

```json
{
  "meetup": [
    { "url": "https://www.meetup.com/bcnrust" },
    {
      "url": "https://www.meetup.com/join-srug",
      "event_format": "hybrid"
    },
    {
      "url": "https://www.meetup.com/hackerdojo",
      "required_title_token": "rust"
    }
  ],
  "luma": [
    {
      "calendar_url": "https://luma.com/rust-girona",
      "calendar_id": "cal-YjQVtnwkdU40fBI"
    }
  ]
}
```

Meetup records support an optional `event_format` of `virtual` or `hybrid` and an optional `required_title_token`. Required title tokens are matched case-insensitively after splitting titles on non-alphanumeric characters. If the OAuth client has multiple signing keys or requires explicit key selection, also set `MEETUP_SIGNING_KEY_ID`.

Luma records contain the canonical public calendar URL and its `cal-` ID. Collection uses Luma's anonymous calendar endpoint to obtain per-event timezones, location types, and structured geographic addresses. Cross-listed events owned by another calendar are omitted. An optional `event_format` can override virtual or hybrid classification.

Add `--in-place` to atomically update the draft. Collection uses the date range declared in the draft. Luma events take precedence over matching Meetup event URLs, and manually supplied `--new-events-file` events are applied last.

## Diagnose collected events

Add `--debug` to log the deserialized Meetup and Luma API responses, filtering decisions, and location normalization to stderr. For example:

```sh
cargo run -- --draft ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md \
  --event-sources groups/rust-event-sources.json \
  --debug
```

The Meetup diagnostics show both the group and venue locations, which one was selected, and the final rendered location. Authentication tokens and signing credentials are not logged.
