use crate::completion::CompletionEngine;
use crate::exec::{Job, dispatch_command, reap};
use crate::line_editor::read_input;
use crate::parser::ParsedCommand;
use std::collections::HashMap;
use std::ops::ControlFlow;

pub struct Shell {
    pathenv: String,
    complete_db: HashMap<String, Vec<String>>,
    jobs: Vec<Job>,
    comp_eng: CompletionEngine,
}

impl Shell {
    pub fn new() -> Shell {
        let pathenv =
            std::env::var("PATH").unwrap_or_else(|_| String::from("/usr/local/bin:/usr/bin:/bin"));
        let complete_db = HashMap::new();
        let jobs = Vec::new();
        let comp_eng = CompletionEngine::new(&pathenv);

        Shell {
            pathenv,
            complete_db,
            jobs,
            comp_eng,
        }
    }

    pub fn dispatch(&mut self, cmd: ParsedCommand) -> ControlFlow<()> {
        dispatch_command(&self.pathenv, cmd, &mut self.complete_db, &mut self.jobs)
    }

    pub fn reap(&mut self) {
        reap(&mut self.jobs)
    }

    pub fn read_line(&self) -> String {
        read_input(&self.comp_eng, &self.complete_db)
    }
}
