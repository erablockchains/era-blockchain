//! Minimal entrypoint – delegate everything to command::run()

#![warn(rust_2018_idioms)]

mod chain_spec;
mod cli;
mod command;
mod rpc;
mod service;

fn main() -> Result<(), Box<sc_cli::Error>> {
    command::run()
}
