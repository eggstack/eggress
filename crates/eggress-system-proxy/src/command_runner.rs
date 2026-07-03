use std::process::Output;

/// Trait for executing system commands, enabling test injection.
///
/// Production code uses `RealCommandRunner`; tests inject
/// `MockCommandRunner` to verify behavior without side effects.
pub trait CommandRunner {
    /// Execute a command with the given arguments and return the output.
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, std::io::Error>;
}

/// Production command runner that executes real system commands.
pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        std::process::Command::new(program).args(args).output()
    }
}

/// Mock command runner for testing.
pub struct MockCommandRunner {
    responses: Vec<(String, Vec<String>, Result<Output, std::io::Error>)>,
    calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
}

impl MockCommandRunner {
    /// Create a new mock with no predefined responses.
    pub fn new() -> Self {
        Self {
            responses: Vec::new(),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Add a predefined response for a command.
    pub fn add_response(
        mut self,
        program: &str,
        args: Vec<String>,
        result: Result<Output, std::io::Error>,
    ) -> Self {
        self.responses.push((program.to_string(), args, result));
        self
    }

    /// Add a response matching any invocation of a program.
    pub fn add_always(self, program: &str, result: Result<Output, std::io::Error>) -> Self {
        self.add_response(program, Vec::new(), result)
    }

    /// Get all recorded calls.
    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for MockCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRunner for MockCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, std::io::Error> {
        let args_vec: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.calls
            .lock()
            .unwrap()
            .push((program.to_string(), args_vec.clone()));

        for (resp_prog, resp_args, resp_result) in &self.responses {
            if resp_prog == program && (resp_args.is_empty() || resp_args == &args_vec) {
                return match resp_result {
                    Ok(output) => Ok(Output {
                        status: output.status,
                        stdout: output.stdout.clone(),
                        stderr: output.stderr.clone(),
                    }),
                    Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
                };
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no mock response for {program}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn success_exit_status() -> std::process::ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(0)
        }
        #[cfg(not(unix))]
        {
            std::process::ExitStatus::default()
        }
    }

    #[test]
    fn mock_runner_returns_predefined_response() {
        let runner = MockCommandRunner::new().add_always(
            "echo",
            Ok(Output {
                status: success_exit_status(),
                stdout: b"hello\n".to_vec(),
                stderr: Vec::new(),
            }),
        );

        let output = runner.run("echo", &["test"]).unwrap();
        assert_eq!(output.stdout, b"hello\n");
    }

    #[test]
    fn mock_runner_records_calls() {
        let runner = MockCommandRunner::new().add_always(
            "ls",
            Ok(Output {
                status: success_exit_status(),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        );

        let _ = runner.run("ls", &["-la"]);
        let _ = runner.run("ls", &["/tmp"]);

        let calls = runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "ls");
        assert_eq!(calls[1].1, vec!["/tmp".to_string()]);
    }

    #[test]
    fn mock_runner_error_for_unknown_command() {
        let runner = MockCommandRunner::new();
        let result = runner.run("nonexistent", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn real_runner_executes_command() {
        let runner = RealCommandRunner;
        let output = runner.run("echo", &["test"]).unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "test");
    }
}
