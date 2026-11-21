mod check;
mod cli;
mod clock;
mod config;
mod coverage;
mod db;
mod embed;
mod finish;
mod hooks;
mod howto;
mod ingest;
mod items;
mod kg;
mod migrate;
mod prs;
mod recall;
mod resolve;
mod search;
mod server;
mod views;

fn main() {
    cli::run();
}
