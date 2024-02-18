use std::net::SocketAddr;

use color_eyre::eyre::{eyre, Context, ContextCompat, Result as RResult};
use pico_args::Arguments;

use crate::{Client, Server};

pub enum Args {
    Help,
    SubCommand { is_quiet: bool, command: SubCommand },
}

pub enum SubCommand {
    Server(Server),
    Client(Client),
}

fn file(args: &mut Arguments) -> RResult<String> {
    args.free_from_str::<String>()
        .context("Missing FILE argument")
}

fn address(args: &mut Arguments) -> RResult<SocketAddr> {
    args.free_from_str::<String>()
        .context("Missing ADDR argument")?
        .parse()
        .context("Failed to parse socket address")
}

pub const HELP: &str = "\
    USAGE: rdev [FLAGS] <COMMAND> <FILE> <ADDR>
    
    Commands:
        build            Watches the file for changes, and sends it to the runner.
        run              Listens for the builder, receives and runs the file.

    Flags:
        -q, --quiet      Tells the program to not output information, except for errors.

        -h, --help       Shows this message.

    File:                When building, it is the file to be sent to the runner.
                         When running, the filename to save to save the file.

    Addr:                When building, it is the address of the builder.
                         When running, the address of the runner.";

pub fn parse_args() -> RResult<Args> {
    let mut args = Arguments::from_env();
    if args.contains(["-h", "--help"]) {
        return Ok(Args::Help);
    }
    let is_quiet = args.contains(["-q", "--quiet"]);
    let subcommand = args.subcommand()?.context("Missing subcommand")?;
    let args = &mut args;
    let command = match &*subcommand {
        "build" => SubCommand::Server(Server {
            file: file(args)?,
            client: address(args)?,
        }),
        "run" => SubCommand::Client(Client {
            file: file(args)?,
            listen: address(args)?,
        }),
        sub => return Err(eyre!("Invalid subcommand {}", sub)),
    };
    Ok(Args::SubCommand { is_quiet, command })
}
