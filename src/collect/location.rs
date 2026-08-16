use crate::events::Region;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Location {
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

impl Location {
    pub fn new(city: Option<String>, state: Option<String>, country: Option<String>) -> Self {
        let city = normalize(city);
        let mut country = normalize(country).map(|value| value.to_uppercase());
        let mut state = normalize(state).map(|value| {
            if country.as_deref() == Some("US") {
                us_state_code(&value).unwrap_or_else(|| value.to_uppercase())
            } else {
                value.to_uppercase()
            }
        });

        if country.as_deref() == Some("GB") {
            country = Some("UK".to_owned());
            state = None;
        }

        Self {
            city,
            state,
            country,
        }
    }

    pub fn fields_present(&self) -> usize {
        [&self.city, &self.state, &self.country]
            .into_iter()
            .filter(|field| field.is_some())
            .count()
    }

    pub fn display_name(&self) -> String {
        let state = if self.country.as_deref() == Some("US") {
            self.state.as_deref()
        } else {
            None
        };
        [self.city.as_deref(), state, self.country.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn region(&self) -> Option<Region> {
        country_region(self.country.as_deref()?)
    }
}

fn normalize(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn us_state_code(state: &str) -> Option<String> {
    let code = match state.trim().to_ascii_lowercase().as_str() {
        "alabama" => "AL",
        "alaska" => "AK",
        "arizona" => "AZ",
        "arkansas" => "AR",
        "california" => "CA",
        "colorado" => "CO",
        "connecticut" => "CT",
        "delaware" => "DE",
        "district of columbia" => "DC",
        "florida" => "FL",
        "georgia" => "GA",
        "hawaii" => "HI",
        "idaho" => "ID",
        "illinois" => "IL",
        "indiana" => "IN",
        "iowa" => "IA",
        "kansas" => "KS",
        "kentucky" => "KY",
        "louisiana" => "LA",
        "maine" => "ME",
        "maryland" => "MD",
        "massachusetts" => "MA",
        "michigan" => "MI",
        "minnesota" => "MN",
        "mississippi" => "MS",
        "missouri" => "MO",
        "montana" => "MT",
        "nebraska" => "NE",
        "nevada" => "NV",
        "new hampshire" => "NH",
        "new jersey" => "NJ",
        "new mexico" => "NM",
        "new york" => "NY",
        "north carolina" => "NC",
        "north dakota" => "ND",
        "ohio" => "OH",
        "oklahoma" => "OK",
        "oregon" => "OR",
        "pennsylvania" => "PA",
        "rhode island" => "RI",
        "south carolina" => "SC",
        "south dakota" => "SD",
        "tennessee" => "TN",
        "texas" => "TX",
        "utah" => "UT",
        "vermont" => "VT",
        "virginia" => "VA",
        "washington" => "WA",
        "west virginia" => "WV",
        "wisconsin" => "WI",
        "wyoming" => "WY",
        code if code.len() == 2
            && code
                .chars()
                .all(|character| character.is_ascii_alphabetic()) =>
        {
            return Some(code.to_ascii_uppercase());
        }
        _ => return None,
    };
    Some(code.to_owned())
}

fn country_region(country: &str) -> Option<Region> {
    match country.to_uppercase().as_str() {
        "AO" | "BF" | "BI" | "BJ" | "BW" | "CD" | "CF" | "CG" | "CI" | "CM" | "CV" | "DJ"
        | "DZ" | "EG" | "ER" | "ET" | "GA" | "GH" | "GM" | "GN" | "GQ" | "GW" | "KE" | "KM"
        | "LR" | "LS" | "LY" | "MA" | "MG" | "ML" | "MR" | "MU" | "MW" | "MZ" | "NA" | "NE"
        | "NG" | "RE" | "RW" | "SC" | "SD" | "SH" | "SL" | "SN" | "SO" | "SS" | "ST" | "SZ"
        | "TD" | "TG" | "TN" | "TZ" | "UG" | "YT" | "ZA" | "ZM" | "ZW" => Some(Region::Africa),
        "AB" | "AE" | "AF" | "AM" | "AZ" | "BD" | "BH" | "BN" | "BT" | "CC" | "CN" | "CX"
        | "CY" | "GE" | "HK" | "ID" | "IL" | "IN" | "IO" | "IQ" | "IR" | "JO" | "JP" | "KG"
        | "KH" | "KP" | "KR" | "KW" | "KZ" | "LA" | "LB" | "LK" | "MM" | "MN" | "MO" | "MV"
        | "MY" | "NP" | "OM" | "OS" | "PH" | "PK" | "PS" | "QA" | "SA" | "SG" | "SY" | "TH"
        | "TJ" | "TM" | "TP" | "TR" | "TW" | "UZ" | "VN" | "YE" => Some(Region::Asia),
        "AD" | "AL" | "AT" | "AX" | "BA" | "BE" | "BG" | "BY" | "CH" | "CZ" | "DE" | "DK"
        | "EE" | "ES" | "FI" | "FO" | "FR" | "GB" | "GG" | "GI" | "GR" | "HR" | "HU" | "IE"
        | "IM" | "IS" | "IT" | "JE" | "LI" | "LT" | "LU" | "LV" | "MC" | "MD" | "ME" | "MK"
        | "MT" | "NL" | "NO" | "PL" | "PT" | "RO" | "RS" | "RU" | "SE" | "SI" | "SJ" | "SK"
        | "SM" | "UA" | "UK" | "XK" => Some(Region::Europe),
        "AG" | "AI" | "AW" | "BB" | "BL" | "BM" | "BQ" | "BS" | "BZ" | "CA" | "CR" | "CU"
        | "CW" | "DM" | "DO" | "GD" | "GL" | "GP" | "GT" | "HN" | "HT" | "JM" | "KN" | "KY"
        | "LC" | "MF" | "MQ" | "MS" | "MX" | "NI" | "PA" | "PM" | "PR" | "SV" | "TC" | "TT"
        | "US" | "VC" | "VG" | "VI" => Some(Region::NorthAmerica),
        "AS" | "AU" | "CK" | "FJ" | "FM" | "GU" | "KI" | "MH" | "MP" | "NC" | "NF" | "NR"
        | "NU" | "NZ" | "PF" | "PG" | "PW" | "SB" | "TK" | "TO" | "TV" | "VU" | "WF" | "WS" => {
            Some(Region::Oceania)
        }
        "AR" | "BO" | "BR" | "CL" | "CO" | "EC" | "FK" | "GF" | "GS" | "GY" | "PE" | "PY"
        | "SR" | "UY" | "VE" => Some(Region::SouthAmerica),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_great_britain_for_twir() {
        let location = Location::new(
            Some("London".to_owned()),
            Some("SW1".to_owned()),
            Some("gb".to_owned()),
        );

        assert_eq!(location.display_name(), "London, UK");
        assert_eq!(location.region(), Some(Region::Europe));
    }

    #[test]
    fn includes_state_for_us_locations() {
        let location = Location::new(
            Some("Indianapolis".to_owned()),
            Some("in".to_owned()),
            Some("us".to_owned()),
        );

        assert_eq!(location.display_name(), "Indianapolis, IN, US");
        assert_eq!(location.region(), Some(Region::NorthAmerica));
    }

    #[test]
    fn abbreviates_us_state_names() {
        let location = Location::new(
            Some("San Francisco".to_owned()),
            Some("California".to_owned()),
            Some("US".to_owned()),
        );

        assert_eq!(location.display_name(), "San Francisco, CA, US");
    }

    #[test]
    fn omits_state_for_non_us_locations() {
        let leipzig = Location::new(
            Some("Leipzig".to_owned()),
            Some("SN".to_owned()),
            Some("DE".to_owned()),
        );
        let montreal = Location::new(
            Some("Montreal".to_owned()),
            Some("QC".to_owned()),
            Some("CA".to_owned()),
        );

        assert_eq!(leipzig.display_name(), "Leipzig, DE");
        assert_eq!(montreal.display_name(), "Montreal, CA");
        assert_eq!(leipzig.fields_present(), 3);
        assert_eq!(montreal.fields_present(), 3);
    }
}
