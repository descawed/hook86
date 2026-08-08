use std::env;
use std::process::Command;

use hook86::dll::{InjectError, inject};

fn main() -> Result<(), InjectError> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <DLL path> <PID or EXE path>", args[0]);
        std::process::exit(1);
    }

    if let Ok(pid) = args[2].parse::<u32>() {
        inject(&args[1], pid)
    } else {
        let mut command = Command::new(&args[2]);
        for arg in args.iter().skip(3) {
            command.arg(arg);
        }

        let process = command.spawn()?;
        inject(&args[1], process.id())
    }
}