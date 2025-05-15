# Lint

Use the linter for the TWIR draft like so:
```
cargo run --bin lint -- -f ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md
```

# Merge
Merge events (output from meetup automation) and the current draft events with the merger:
```
cargo run --bin merge -- -f ../this-week-in-rust/draft/2025-05-14-this-week-in-rust.md -n ~/scratch/14may
```
The second file here (`-n` arg) is the output of the meetup automation python script. The output of this is the new event section
which can be copy pasted into the draft.
