use std::io::{self, BufRead, Write};

use video_worker::{handle_request, hello, parse_mode, WorkerMode, WorkerRequest};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match parse_mode(&args) {
        WorkerMode::Ping => print_ping(),
        WorkerMode::Stdio => run_stdio(),
    };

    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn print_ping() -> Result<(), String> {
    let payload = serde_json::to_string(&hello(true)).map_err(|error| error.to_string())?;
    println!("{payload}");
    Ok(())
}

fn run_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }

        let request: WorkerRequest =
            serde_json::from_str(&line).map_err(|error| format!("invalid request: {error}"))?;
        let should_shutdown = matches!(request, WorkerRequest::Shutdown);
        let response =
            serde_json::to_string(&handle_request(request)).map_err(|error| error.to_string())?;

        writeln!(stdout, "{response}").map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())?;

        if should_shutdown {
            break;
        }
    }

    Ok(())
}
