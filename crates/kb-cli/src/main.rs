use std::{collections::BTreeMap, process::ExitCode};

use kb_app::AppContext;
use kb_core::KbError;

mod args;
mod render;

fn main() -> ExitCode {
    let parsed = args::parse();
    let current_dir = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            return render::error(
                KbError::io_failure("read current directory", ".", error.to_string()),
                parsed.json,
            );
        }
    };
    let context = AppContext::new(std::env::vars().collect::<BTreeMap<_, _>>(), current_dir);
    match kb_app::run(parsed.request, &context) {
        Ok(value) => render::success(&value, parsed.json),
        Err(error) => render::error(error, parsed.json),
    }
}
