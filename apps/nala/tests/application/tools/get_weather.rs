use nala::ports::http::HttpError;

use crate::fake_http_fetcher::FakeHttpFetcher;
use nala::application::tools::Tool;
use nala::application::tools::get_weather::{GetWeatherArgs, GetWeatherTool};

const GEOCODING_RESPONSE: &str =
    r#"{"results":[{"latitude":40.4168,"longitude":-3.7038,"name":"Madrid"}]}"#;
const FORECAST_RESPONSE: &str = r#"{"current":{"temperature_2m":22.5,"relative_humidity_2m":40,"weather_code":1,"wind_speed_10m":10.0}}"#;
const EMPTY_GEOCODING_RESPONSE: &str = r#"{"results":[]}"#;

#[test]
fn reports_weather_for_a_found_location() {
    let fetcher = FakeHttpFetcher::with_responses(vec![
        Ok(GEOCODING_RESPONSE.to_string()),
        Ok(FORECAST_RESPONSE.to_string()),
    ]);
    let mut tool = GetWeatherTool::new(fetcher);

    let result = tool
        .execute(GetWeatherArgs {
            location: "Madrid".to_string(),
        })
        .expect("get_weather should not fail");

    assert!(result.contains("22"));
    assert!(result.contains("Madrid"));
}

#[test]
fn reports_a_clear_message_when_location_is_not_found() {
    let fetcher = FakeHttpFetcher::with_responses(vec![Ok(EMPTY_GEOCODING_RESPONSE.to_string())]);
    let mut tool = GetWeatherTool::new(fetcher);

    let result = tool
        .execute(GetWeatherArgs {
            location: "Nowhereville".to_string(),
        })
        .expect("get_weather should not fail even when nothing is found");

    assert!(
        result.to_lowercase().contains("no enconté")
            || result.to_lowercase().contains("no encontré")
    );
}

#[test]
fn requests_the_expected_geocoding_url() {
    let fetcher = FakeHttpFetcher::with_responses(vec![
        Ok(GEOCODING_RESPONSE.to_string()),
        Ok(FORECAST_RESPONSE.to_string()),
    ]);
    let mut tool = GetWeatherTool::new(fetcher);

    tool.execute(GetWeatherArgs {
        location: "Madrid".to_string(),
    })
    .unwrap();

    let urls = tool.fetcher.requested_urls.borrow();
    assert!(urls[0].contains("geocoding-api.open-meteo.com"));
    assert!(urls[0].contains("Madrid"));
    assert!(urls[1].contains("api.open-meteo.com"));
}

#[test]
fn propagates_http_errors() {
    let fetcher = FakeHttpFetcher::with_responses(vec![Err(HttpError::Status(500))]);
    let mut tool = GetWeatherTool::new(fetcher);

    let result = tool.execute(GetWeatherArgs {
        location: "Madrid".to_string(),
    });

    assert!(result.is_err());
}
