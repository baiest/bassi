use crate::fake_computer::FakeComputer;
use nala::application::tools::Tool;
use nala::application::tools::volume::{VolumeArgs, VolumeTool};
use schemars::schema_for;

#[test]
fn turns_volume_up() {
    let computer = FakeComputer::new();
    let mut tool: VolumeTool<FakeComputer> = VolumeTool::new(computer);

    let args = VolumeArgs {
        action: "up".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.executed_command,
        Some(
            r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]175)""#
                .to_string()
        )
    );
}

#[test]
fn turns_volume_down() {
    let computer = FakeComputer::new();
    let mut tool: VolumeTool<FakeComputer> = VolumeTool::new(computer);

    let args = VolumeArgs {
        action: "down".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.executed_command,
        Some(
            r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]174)""#
                .to_string()
        )
    );
}

#[test]
fn toggles_mute() {
    let computer = FakeComputer::new();
    let mut tool: VolumeTool<FakeComputer> = VolumeTool::new(computer);

    let args = VolumeArgs {
        action: "mute".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.executed_command,
        Some(
            r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]173)""#
                .to_string()
        )
    );
}

#[test]
fn rejects_an_unknown_action_without_touching_the_computer() {
    let computer = FakeComputer::new();
    let mut tool: VolumeTool<FakeComputer> = VolumeTool::new(computer);

    let args = VolumeArgs {
        action: "unmute".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
    assert_eq!(tool.computer.executed_command, None);
}

#[test]
fn returns_error_when_computer_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;
    let mut tool = VolumeTool::new(computer);

    let args = VolumeArgs {
        action: "up".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
}

#[test]
fn published_schema_matches_the_derived_schema_of_args() {
    let expected =
        serde_json::to_value(schema_for!(VolumeArgs)).expect("schema should serialize to JSON");

    let definition = VolumeTool::<FakeComputer>::definition();

    assert_eq!(definition.parameters, expected);
}
