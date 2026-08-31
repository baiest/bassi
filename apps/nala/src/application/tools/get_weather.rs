use schemars::JsonSchema;
use serde::Deserialize;

use crate::application::tools::Tool;
use crate::ports::http::{HttpError, HttpFetcher};

#[derive(Deserialize, JsonSchema)]
pub struct GetWeatherArgs {
    /// The place to get the weather for, e.g. a city name.
    pub location: String,
}

pub struct GetWeatherTool<H: HttpFetcher> {
    pub fetcher: H,
}

impl<H: HttpFetcher> GetWeatherTool<H> {
    pub fn new(fetcher: H) -> Self {
        Self { fetcher }
    }
}

#[derive(serde::Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Vec<GeocodingResult>,
}

#[derive(serde::Deserialize)]
struct GeocodingResult {
    latitude: f64,
    longitude: f64,
    name: String,
}

#[derive(serde::Deserialize)]
struct ForecastResponse {
    current: CurrentWeather,
}

#[derive(serde::Deserialize)]
struct CurrentWeather {
    temperature_2m: f64,
    relative_humidity_2m: f64,
    weather_code: u32,
    wind_speed_10m: f64,
}

/// Maps a subset of WMO weather codes (used by Open-Meteo) to a short
/// Spanish description. Not exhaustive — falls back to a generic phrase for
/// codes not explicitly handled.
fn describe_weather_code(code: u32) -> &'static str {
    match code {
        0 => "cielo despejado",
        1 | 2 => "parcialmente nublado",
        3 => "nublado",
        45 | 48 => "niebla",
        51 | 53 | 55 => "llovizna",
        61 | 63 | 65 => "lluvia",
        71 | 73 | 75 => "nieve",
        80..=82 => "chubascos",
        95 | 96 | 99 => "tormenta",
        _ => "condiciones variables",
    }
}

impl<H: HttpFetcher> Tool for GetWeatherTool<H> {
    type Args = GetWeatherArgs;
    type Output = String;
    type Error = HttpError;

    const NAME: &'static str = "get_weather";
    const DESCRIPTION: &'static str = "Get the current weather for a given location (city name). Returns temperature, humidity, wind, and general conditions.";

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(GetWeatherArgs))
            .expect("GetWeatherArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let geocoding_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=es",
            urlencode(&args.location)
        );
        let geocoding_body = self.fetcher.get(&geocoding_url)?;
        let geocoding: GeocodingResponse = serde_json::from_str(&geocoding_body)
            .map_err(|error| HttpError::Request(error.to_string()))?;

        let Some(place) = geocoding.results.into_iter().next() else {
            return Ok(format!(
                "No encontré esa ubicación ({}). Prueba con otro nombre.",
                args.location
            ));
        };

        let forecast_url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m&timezone=auto",
            place.latitude, place.longitude
        );
        let forecast_body = self.fetcher.get(&forecast_url)?;
        let forecast: ForecastResponse = serde_json::from_str(&forecast_body)
            .map_err(|error| HttpError::Request(error.to_string()))?;

        Ok(format!(
            "En {} hace {:.0}°C con {} (humedad {:.0}%, viento {:.0} km/h).",
            place.name,
            forecast.current.temperature_2m,
            describe_weather_code(forecast.current.weather_code),
            forecast.current.relative_humidity_2m,
            forecast.current.wind_speed_10m,
        ))
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| HttpError::Request(error.to_string()))
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}

fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
