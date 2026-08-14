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
      "ical_url": "https://api.lu.ma/ics/get?entity=calendar&id=cal-YjQVtnwkdU40fBI",
      "default_location": "Girona, ES",
      "region": "Europe",
      "timezone": "Europe/Madrid"
    }
  ]
}
```

Meetup records support an optional `event_format` of `virtual` or `hybrid` and an optional `required_title_token`. Required title tokens are matched case-insensitively after splitting titles on non-alphanumeric characters. If the OAuth client has multiple signing keys or requires explicit key selection, also set `MEETUP_SIGNING_KEY_ID`.

Copy a Luma calendar's feed URL from **Add iCal Subscription → Copy URL** on its public page. Its records require a TWIR location, region, and IANA timezone because iCalendar locations are unstructured and timestamps may be UTC. An optional `event_format` can override virtual or hybrid classification; otherwise a URL-valued iCalendar `LOCATION` is treated as virtual.

Add `--in-place` to atomically update the draft. Collection uses the date range declared in the draft. Luma events take precedence over matching Meetup event URLs, and manually supplied `--new-events-file` events are applied last.
