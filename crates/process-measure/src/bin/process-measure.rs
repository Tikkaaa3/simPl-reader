use process_measure::args::{self, Parsed};

fn main() {
    let argv: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = match args::parse(&argv) {
        Ok(Parsed::Help) => {
            print!("{}", args::usage_text());
            0
        }
        Ok(Parsed::Run(args)) => {
            let result = process_measure::run::run_collection(*args);
            if result.exit_code == 0 {
                println!("{}", result.message);
            } else {
                eprintln!("error: {}", result.message);
            }
            result.exit_code
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{}", args::usage_text());
            2
        }
    };
    std::process::exit(code);
}
