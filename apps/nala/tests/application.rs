#[path = "common/fake_computer.rs"]
mod fake_computer;

#[path = "common/fake_llm.rs"]
mod fake_llm;

#[path = "common/fake_mcp.rs"]
mod fake_mcp;

#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "common/fake_clock.rs"]
mod fake_clock;

#[path = "common/fake_wall_clock.rs"]
mod fake_wall_clock;

#[path = "common/fake_http_fetcher.rs"]
mod fake_http_fetcher;

#[path = "common/fake_cancel.rs"]
mod fake_cancel;

#[path = "common/fake_device.rs"]
mod fake_device;

#[path = "application/tools/device_toolset.rs"]
mod device_toolset;

#[path = "application/tools/registry.rs"]
mod registry;

#[path = "application/tools/ping.rs"]
mod ping;

#[path = "application/tools/current_time.rs"]
mod current_time;

#[path = "application/tools/get_weather.rs"]
mod get_weather;

#[path = "application/tools/web_search.rs"]
mod web_search;

#[path = "application/tools/fetch_url.rs"]
mod fetch_url;

#[path = "application/tools/dispatcher.rs"]
mod dispatcher;

#[path = "application/tools/mcp_toolset.rs"]
mod mcp_toolset;

#[path = "application/tools/remember.rs"]
mod remember;

#[path = "application/assistant.rs"]
mod assistant;

#[path = "application/memory.rs"]
mod memory;

#[path = "application/context_budget.rs"]
mod context_budget;
