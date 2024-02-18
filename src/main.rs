use std::io::Write;
use std::path::PathBuf;
use std::{collections::HashSet, convert::Infallible, ffi::OsStr};
use std::{os::unix::fs::OpenOptionsExt, path::Path, process::ExitCode};
use std::{fs::{File, OpenOptions}, io::{BufReader, BufWriter}, net::{SocketAddr, TcpListener, TcpStream}, process::Command,};
use color_eyre::eyre::{ eyre, Context, ContextCompat };
use flate2::{bufread::{ZlibDecoder, ZlibEncoder}, Compression};
use inotify::{WatchDescriptor, WatchMask};

use color_eyre::Result as RResult;

struct Server {
    client: SocketAddr,
    file: String
}

struct Client {
    listen: SocketAddr,
    file: String
}

struct Watcher {
    inotify: inotify::Inotify,
    buf: Box<[u8]>,
    folder: PathBuf,
    watch_descriptor: Option<WatchDescriptor>,
}

fn dirname(path: &dyn AsRef<Path>) -> &Path {
    path.as_ref().parent().unwrap_or(Path::new("."))
}

impl Watcher {
    fn new(folder: &Path) -> RResult<Watcher> {
        let inotify = inotify::Inotify::init()?;
        let buf = Box::new([0; 2048]);
        Ok(Self {
            folder: folder.into(),
            buf,
            inotify,
            watch_descriptor: None
        })
    }

    fn start_watching(&mut self) -> RResult<()> {
        if self.watch_descriptor.is_none() {
            self.watch_descriptor = Some(self.inotify.watches().add(&self.folder, WatchMask::CLOSE | WatchMask::MOVED_TO | WatchMask::ATTRIB )?);
        }
        Ok(())
    }

    fn stop_watching(&mut self) -> RResult<()> {
        if let Some(wd) = self.watch_descriptor.take() {
            self.inotify.watches().remove(wd)?
        }
        Ok(())
    }

    fn files_changed(&mut self) -> RResult<HashSet<&OsStr>> {
        Ok(self.inotify.read_events_blocking(&mut *self.buf)?.filter_map(|e| e.name).collect())
    }
}

fn strip(file: &Path, output: Option<&Path>) -> RResult<File> {
    let mut cmd = Command::new("strip");
    if let Some(output) = output {
        cmd.args(["-o".as_ref(), output.as_os_str()]);
    }
    cmd.arg(file).spawn()?.wait()?;
    Ok(File::open(output.unwrap_or(file))?)
}



fn server(Server { file, client }: Server) -> RResult<Infallible> {
    let file = Path::new(&file);
    let filename = file.file_name().context("Expected file name, got '..'")?;
    let mut watcher = Watcher::new(dirname(&file))?;
    watcher.start_watching()?;
    loop {
        if watcher.files_changed()?.contains(filename) {
            watcher.stop_watching()?;
            let file = strip(file, Some(Path::new("/tmp/tempfile2")))?;
            let file = BufReader::new(file);
            let mut encoder = ZlibEncoder::new(file, Compression::fast());
            let mut tcp = BufWriter::new(TcpStream::connect(client.clone())?);
            log::info!("Sending file...");
            std::io::copy(&mut encoder, &mut tcp)?;
            tcp.flush()?;
            log::info!("Done!");
            watcher.start_watching()?;
        }
    }
}

fn client(Client { file, listen }: Client) -> RResult<Infallible> {
    let file_path = Path::new(&file);
    let listen = TcpListener::bind(listen)?;
    let mut open_options = OpenOptions::new();
    open_options.create(true).write(true).mode(0o766);
    loop {
        {
            let (con, _) = listen.accept()?;
            log::info!("Got file...");
            let mut con = ZlibDecoder::new(BufReader::new(con));
            let mut file = BufWriter::new(open_options.open(file_path)?);
            log::info!("Decompressing...");
            std::io::copy(&mut con, &mut file)?;
        }
        log::info!("Running...");
        Command::new(file_path).spawn()?.wait()?;
    }
}

fn run(args: SubCommand) -> RResult<Infallible> {
    match args {
        SubCommand::Client(c) => client(c),
        SubCommand::Server(s) => server(s),
    }
}


const HELP: &str = "\
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

enum Args {
    Help,
    SubCommand { is_quiet: bool, command: SubCommand }
}

enum SubCommand {
    Server(Server),
    Client(Client)
}

fn parse_args() -> RResult<Args> {
    let mut args = pico_args::Arguments::from_env();
    if args.contains(["-h", "--help"]) {
        return Ok(Args::Help)
    }
    let is_quiet = args.contains(["-q", "--quiet"]);
    let subcommand = args.subcommand()?.context("Missing subcommand")?;
    let file = args.free_from_str().context("Missing FILE argument")?;
    let addr = args.free_from_str().context("Missing ADDR argument")?;
    let command = match &*subcommand {
        "build" => SubCommand::Server(Server{ file, client: addr }),
        "run" => SubCommand::Client(Client { file, listen: addr }),
        sub => return Err(eyre!("Invalid subcommand {}", sub))
    };
    Ok(Args::SubCommand { is_quiet, command })
} 

fn main() -> ExitCode {
  color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install().unwrap();
    let args = match parse_args() {
        Ok(it) => it,
        Err(err) => {
            eprintln!("Error: {}\n\n{}", err, HELP);
            return ExitCode::FAILURE
        },
    };
    match args {
        Args::Help => println!("{}", HELP),
        Args::SubCommand { is_quiet, command } => {
            let level = if is_quiet { log::LevelFilter::Info } else { log::LevelFilter::Error };
            env_logger::builder().filter_level(level).init();
            if let Err(err) = run(command) {
                eprintln!("Error: {:?}", err);
                return ExitCode::FAILURE
            }
        }
    }
    ExitCode::SUCCESS
}
