use std::{ffi::{OsStr, OsString}, net::SocketAddr};

use color_eyre::eyre::{eyre, Context, ContextCompat, Result as RResult};
use pico_args::Arguments;

use crate::{Receiver, RunAction, Sender};

pub enum Args {
    Help,
    SubCommand { is_quiet: bool, command: SubCommand },
}

pub enum SubCommand {
    Server(Sender),
    Client(Receiver),
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

fn parse_action(value: &str) -> RResult<RunAction> {
    let (verb, arg): (&str, Option<&str>) = if value.contains(":") {
        todo!()
    } else {
        todo!()
    };
    if value == "recv" {
        Ok(RunAction::ReceivedFile)
    } else {

        Ok(RunAction::Script(value.to_owned()))
    }
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
    let action = args.opt_value_from_fn(["-a", "--action"], parse_action)?;

    let subcommand = args.subcommand()?.context("Missing subcommand")?;
    let args = &mut args;
    let command = match &*subcommand {
        "build" => SubCommand::Server(Sender {
            file: file(args)?,
            receiver_addr: address(args)?,
        }),
        "run" => SubCommand::Client(Receiver {
            file: file(args)?,
            listen: address(args)?,
            on_receive: todo!(),
        }),
        sub => return Err(eyre!("Invalid subcommand {}", sub)),
    };
    Ok(Args::SubCommand { is_quiet, command })
}
