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
        let mut state = normalize(state).map(|value| value.to_uppercase());
        let mut country = normalize(country).map(|value| value.to_uppercase());

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
        [&self.city, &self.state, &self.country]
            .into_iter()
            .filter_map(|field| field.as_deref())
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
    fn joins_only_present_location_fields() {
        let location = Location::new(None, Some("ca".to_owned()), Some("us".to_owned()));

        assert_eq!(location.display_name(), "CA, US");
        assert_eq!(location.region(), Some(Region::NorthAmerica));
    }
}
