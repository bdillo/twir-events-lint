use std::{thread, time::Duration};

use anyhow::{Context, bail};
use chrono::{DateTime, NaiveDate, Utc};
use log::{debug, warn};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use super::auth::MeetupCredentials;

const AUTH_ENDPOINT: &str = "https://secure.meetup.com/oauth2/access";
const GRAPHQL_ENDPOINT: &str = "https://api.meetup.com/gql-ext";
const EVENTS_PER_PAGE: u16 = 20;
const MAX_RATE_LIMIT_RETRIES: usize = 1;
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(120);

const EVENT_LISTING_QUERY: &str = r#"
query(
  $first: Int!,
  $after: String,
  $urlName: String!,
  $afterDateTime: DateTime!,
  $beforeDateTime: DateTime!
) {
  groupByUrlname(urlname: $urlName) {
    events(
      first: $first,
      after: $after,
      sort: ASC,
      filter: {
        afterDateTime: $afterDateTime,
        beforeDateTime: $beforeDateTime,
        status: [ACTIVE]
      }
    ) {
      pageInfo {
        hasNextPage
        endCursor
      }
      edges {
        node {
          group {
            name
            city
            state
            country
          }
          title
          dateTime
          eventUrl
          venues {
            city
            state
            country
            venueType
          }
        }
      }
    }
  }
}
"#;

pub struct MeetupClient {
    http: Client,
}

pub struct AccessToken(String);

#[derive(Debug)]
pub struct EventPage {
    pub events: Vec<ApiEvent>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEvent {
    pub group: Option<ApiGroup>,
    pub title: Option<String>,
    pub date_time: Option<String>,
    pub event_url: Option<String>,
    #[serde(default)]
    pub venues: Vec<ApiVenue>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ApiGroup {
    pub name: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiVenue {
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub venue_type: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EventVariables<'a> {
    first: u16,
    after: Option<&'a str>,
    url_name: &'a str,
    after_date_time: String,
    before_date_time: String,
}

#[derive(Serialize)]
struct GraphqlRequest<'a> {
    query: &'static str,
    variables: EventVariables<'a>,
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphqlError>,
}

#[derive(Debug, Deserialize)]
struct GraphqlError {
    message: String,
    extensions: Option<GraphqlErrorExtensions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphqlErrorExtensions {
    code: Option<String>,
    reset_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventData {
    group_by_urlname: Option<ApiEventConnection>,
}

#[derive(Debug, Deserialize)]
struct ApiEventConnection {
    events: ApiEvents,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEvents {
    page_info: PageInfo,
    edges: Vec<ApiEventEdge>,
}

#[derive(Debug, Deserialize)]
struct ApiEventEdge {
    node: ApiEvent,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

fn graphql_range_start(start: NaiveDate) -> String {
    let start = start.pred_opt().unwrap_or(start);
    format!("{start}T00:00:00Z")
}

fn graphql_range_end(end: NaiveDate) -> String {
    let end = end.succ_opt().unwrap_or(end);
    format!("{end}T23:59:59Z")
}

impl MeetupClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("twir-events-lint/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to build Meetup HTTP client")?;
        Ok(Self { http })
    }

    pub fn authenticate(&self, credentials: &MeetupCredentials) -> anyhow::Result<AccessToken> {
        let assertion = credentials.signed_jwt()?;
        let response = self
            .http
            .post(AUTH_ENDPOINT)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .context("failed to request Meetup access token")?
            .error_for_status()
            .context("meetup authentication failed")?
            .json::<TokenResponse>()
            .context("failed to parse Meetup authentication response")?;

        if response.access_token.is_empty() {
            bail!("meetup authentication response contained an empty access token");
        }
        Ok(AccessToken(response.access_token))
    }

    pub fn event_page(
        &self,
        token: &AccessToken,
        group_name: &str,
        after: Option<&str>,
        range_start: NaiveDate,
        range_end: NaiveDate,
    ) -> anyhow::Result<Option<EventPage>> {
        let request = GraphqlRequest {
            query: EVENT_LISTING_QUERY,
            variables: EventVariables {
                first: EVENTS_PER_PAGE,
                after,
                url_name: group_name,
                after_date_time: graphql_range_start(range_start),
                before_date_time: graphql_range_end(range_end),
            },
        };
        for attempt in 0..=MAX_RATE_LIMIT_RETRIES {
            let response = self
                .http
                .post(GRAPHQL_ENDPOINT)
                .bearer_auth(&token.0)
                .json(&request)
                .send()
                .with_context(|| format!("failed to fetch Meetup group '{group_name}'"))?
                .error_for_status()
                .with_context(|| format!("meetup request failed for group '{group_name}'"))?
                .json::<GraphqlResponse<EventData>>()
                .with_context(|| {
                    format!("failed to parse Meetup response for group '{group_name}'")
                })?;

            debug!(
                "meetup API response for group '{group_name}' (cursor {after:?}): {response:#?}"
            );

            let Some(reset_at) = response.rate_limit_reset() else {
                return parse_event_page(response, group_name);
            };
            if attempt == MAX_RATE_LIMIT_RETRIES {
                bail!("meetup rate limit persisted after retrying group '{group_name}'");
            }
            let wait = rate_limit_wait(reset_at)?;
            warn!(
                "meetup rate limit reached while fetching '{group_name}'; retrying in {} seconds",
                wait.as_secs_f32()
            );
            thread::sleep(wait);
        }

        unreachable!("rate-limit retry loop always returns or fails")
    }
}

impl<T> GraphqlResponse<T> {
    fn rate_limit_reset(&self) -> Option<&str> {
        self.errors.iter().find_map(|error| {
            let extensions = error.extensions.as_ref()?;
            (extensions.code.as_deref() == Some("RATE_LIMITED"))
                .then_some(extensions.reset_at.as_deref())
                .flatten()
        })
    }
}

fn rate_limit_wait(reset_at: &str) -> anyhow::Result<Duration> {
    let reset_at = DateTime::parse_from_rfc3339(reset_at)
        .context("meetup rate-limit response contained an invalid resetAt timestamp")?
        .with_timezone(&Utc);
    let wait = (reset_at - Utc::now())
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(1))
        + Duration::from_millis(250);
    if wait > MAX_RATE_LIMIT_WAIT {
        bail!("meetup rate-limit reset time is more than two minutes away");
    }
    Ok(wait)
}

fn parse_event_page(
    response: GraphqlResponse<EventData>,
    group_name: &str,
) -> anyhow::Result<Option<EventPage>> {
    let data = parse_graphql_data(response, group_name)?;
    let Some(connection) = data.group_by_urlname else {
        return Ok(None);
    };
    let events = connection.events;
    if events.page_info.has_next_page && events.page_info.end_cursor.is_none() {
        bail!("meetup response for '{group_name}' omitted its next-page cursor");
    }

    Ok(Some(EventPage {
        events: events.edges.into_iter().map(|edge| edge.node).collect(),
        next_cursor: events
            .page_info
            .has_next_page
            .then_some(events.page_info.end_cursor)
            .flatten(),
    }))
}

fn parse_graphql_data<T>(response: GraphqlResponse<T>, operation: &str) -> anyhow::Result<T> {
    if !response.errors.is_empty() {
        let messages = response
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        bail!("meetup GraphQL errors for '{operation}': {messages}");
    }
    response
        .data
        .ok_or_else(|| anyhow::anyhow!("meetup response for '{operation}' contained no data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_range_is_widened_for_local_time_zones() {
        let start = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 6, 11).unwrap();

        assert_eq!(graphql_range_start(start), "2026-05-13T00:00:00Z");
        assert_eq!(graphql_range_end(end), "2026-06-12T23:59:59Z");
    }

    #[test]
    fn parses_event_page_and_cursor() {
        let response: GraphqlResponse<EventData> = serde_json::from_str(
            r#"{
                "data": {
                    "groupByUrlname": {
                        "events": {
                            "pageInfo": {"hasNextPage": true, "endCursor": "next"},
                            "edges": [{
                                "node": {
                                    "group": {
                                        "name": "Test Rust",
                                        "city": "Berlin",
                                        "state": null,
                                        "country": "DE"
                                    },
                                    "title": "Test Event",
                                    "dateTime": "2026-05-14T19:00+02:00",
                                    "eventUrl": "https://www.meetup.com/test-rust/events/1/",
                                    "venues": [{
                                        "city": "Berlin",
                                        "state": null,
                                        "country": "DE",
                                        "venueType": "physical"
                                    }]
                                }
                            }]
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let page = parse_event_page(response, "test-rust").unwrap().unwrap();

        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].title.as_deref(), Some("Test Event"));
        assert_eq!(page.next_cursor.as_deref(), Some("next"));
    }

    #[test]
    fn reports_graphql_errors() {
        let response: GraphqlResponse<EventData> =
            serde_json::from_str(r#"{"data": null, "errors": [{"message": "not authorized"}]}"#)
                .unwrap();

        let error = parse_event_page(response, "test-rust").unwrap_err();

        assert!(error.to_string().contains("not authorized"));
    }

    #[test]
    fn recognizes_documented_rate_limit_errors() {
        let response: GraphqlResponse<EventData> = serde_json::from_str(
            r#"{
                "data": null,
                "errors": [{
                    "message": "Too many requests, please try again shortly.",
                    "extensions": {
                        "code": "RATE_LIMITED",
                        "resetAt": "2026-12-12T18:37:51.644Z"
                    }
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(
            response.rate_limit_reset(),
            Some("2026-12-12T18:37:51.644Z")
        );
    }
}
