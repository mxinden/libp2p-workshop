use clap::Parser;
use std::error::Error;

#[derive(Debug, Parser)]
#[clap(name = "libp2p-workshop-node")]
struct Opts {}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let _opts = Opts::parse();

    println!("Hello, world!");

    Ok(())
}
