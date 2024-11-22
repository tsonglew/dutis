use std::env;
use std::error::Error;
use std::process;

#[derive(Debug)]
struct Config {
    content_type: String,
}

impl Config {
    fn new(content_type: String) -> Config {
        Config { content_type }
    }

    fn build(args: Vec<String>) -> Result<Config, &'static str> {
        if args.len() < 2 {
            return Err("not enough arguments");
        }
        Ok(Config::new(args[1].clone()))
    }
}

fn main() {
    let args = env::args().collect::<Vec<String>>();
    let conf = Config::build(args).unwrap_or_else(|err| {
        eprintln!("Problem parsing arguments: {}", err);
        process::exit(1);
    });
    if let Err(e) = run(&conf) {
        eprintln!("Application error: {}", e);
        process::exit(1);
    }
    println!("{:?}", conf);
}

fn run(conf: &Config) -> Result<(), Box<dyn Error>> {
    println!("{:?}", conf);
    Ok(())
}
