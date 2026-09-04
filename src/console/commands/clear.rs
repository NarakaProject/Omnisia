use super::super::command::{CommandResult, ConsoleCommand};
use super::super::context::DeveloperExecutionContext;

/// Console command to clear output lines.
pub struct ClearCommand;

impl ConsoleCommand for ClearCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn description(&self) -> &'static str {
        "Clears the developer console scrollback output buffer."
    }

    fn usage(&self) -> &'static str {
        "clear"
    }

    fn execute(&self, _args: &[String], _ctx: &mut DeveloperExecutionContext) -> CommandResult {
        CommandResult::Clear
    }
}
