use cli::{parse_args, Args, SubCommand};
use color_eyre::eyre::{eyre, Context, ContextCompat};
use flate2::{
    bufread::{ZlibDecoder, ZlibEncoder},
    Compression,
};
use inotify::{WatchDescriptor, WatchMask};
use std::io::Write;
use std::path::PathBuf;
use std::{collections::HashSet, convert::Infallible, ffi::OsStr};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    net::{SocketAddr, TcpListener, TcpStream},
    process::Command,
};
use std::{os::unix::fs::OpenOptionsExt, path::Path, process::ExitCode};

use color_eyre::Result as RResult;

struct Server {
    client: SocketAddr,
    file: String,
}

struct Client {
    listen: SocketAddr,
    file: String,
}

struct Watcher {
    inotify: inotify::Inotify,
    buf: Box<[u8]>,
    folder: PathBuf,
    watch_descriptor: Option<WatchDescriptor>,
}

fn dirname(path: &dyn AsRef<Path>) -> &Path {
    path.as_ref().parent().and_then(|i| {
        if i == Path::new("") {
            None
        } else {
            Some(i)
        }
    }).unwrap_or(Path::new("."))
}

impl Watcher {
    fn new(folder: &Path) -> RResult<Watcher> {
        let inotify = inotify::Inotify::init()?;
        let buf = Box::new([0; 2048]);
        Ok(Self {
            folder: folder.into(),
            buf,
            inotify,
            watch_descriptor: None,
        })
    }

    fn start_watching(&mut self) -> RResult<()> {
        if self.watch_descriptor.is_none() {
            self.watch_descriptor = Some(self.inotify.watches().add(
                &self.folder,
                WatchMask::CLOSE | WatchMask::MOVED_TO | WatchMask::ATTRIB,
            )?);
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
        Ok(self
            .inotify
            .read_events_blocking(&mut *self.buf)?
            .filter_map(|e| e.name)
            .collect())
    }
}

/// Removes debug information from a file `file`, saves it at the path `output` and returns a
/// newly-opened stripped [`File`].
fn strip(file: &Path, output: &Path) -> RResult<File> {
    Command::new("strip")
        .args(["-o".as_ref(), output.as_os_str()])
        .arg(file)
        .spawn()?
        .wait()?;
    Ok(File::open(output)?)
}

fn server(Server { file, client }: Server) -> RResult<Infallible> {
    let file_path = Path::new(&file);
    let filename = file_path.file_name().context("Expected file name, got '..'")?;

    let tmp_filename = Path::new("/tmp/").join(filename);

    let mut watcher = Watcher::new(dirname(&file_path))?;
    watcher.start_watching()?;
    loop {
        // Check if the watched file was changed
        if watcher.files_changed()?.contains(filename) {
            // Stop watching for changes temporarily to prevent the event from getting
            // re-triggered. This is needed because we open the file in the following lines.
            watcher.stop_watching()?;
            // The only step that can't be done natively as a `Read` wrapper.
            let file = strip(file_path, &tmp_filename)?;

            // After striping finishes, we wrap the file in buffered reader and compresser.
            let file = BufReader::new(file);
            let mut encoder = ZlibEncoder::new(file, Compression::fast());

            // Then, we connect to the runner.
            let mut tcp = BufWriter::new(TcpStream::connect(client.clone())?);

            log::info!("Sending file...");
            // And we send the file. This operation sends the compressed output.
            std::io::copy(&mut encoder, &mut tcp)?;

            // We then flush the stream to make sure it's done.
            tcp.flush()?;
            log::info!("Done!");
            // And finally, we begin watching for changes again
            watcher.start_watching()?;
        }
    }
}

fn client(Client { file, listen }: Client) -> RResult<Infallible> {
    let file_path = Path::new(&file);
    // We do this to make sure `file` has a "." if it is just a name.
    let file_path = dirname(&file_path).join(file_path.file_name().context("Expected filename, got ..")?);

    // Listen to incoming requests from the server
    let listen = TcpListener::bind(listen)?;
    log::info!("Listening for connections...");

    // Configure open_options to create a file with the executable bit set
    let mut open_options = OpenOptions::new();
    open_options.create(true).write(true).mode(0o766);
    loop {
        let (con, _) = listen.accept()?;
        log::info!("Got file...");
        let mut con = ZlibDecoder::new(BufReader::new(con));
        let mut file = BufWriter::new(open_options.open(&file_path)?);
        log::info!("Decompressing...");
        // In one fell swoop, we receive, decompress and write to the file.
        // It's way faster than doing it one at a time.
        std::io::copy(&mut con, &mut file)?;

        // Don't forget to drop the file. Otherwise, changes won't be synced and running can fail.
        std::mem::drop(file);

        let mut command = Command::new(&file_path);
        log::info!("Running {:?}...", command);
        command.spawn()?.wait()?;
    }
}

fn run(args: SubCommand) -> RResult<Infallible> {
    match args {
        SubCommand::Client(c) => client(c),
        SubCommand::Server(s) => server(s),
    }
}

mod cli;
use cli::HELP;

fn main() -> ExitCode {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install()
        .unwrap();
    // this could use a let-else but I need to have access to the error.
    let args = match parse_args() {
        Ok(it) => it,
        Err(err) => {
            eprintln!("Error: {}\n\n{}", err, HELP);
            return ExitCode::FAILURE;
        }
    };
    match args {
        Args::Help => println!("{}", HELP),
        Args::SubCommand { is_quiet, command } => {
            let level = if is_quiet {
                log::LevelFilter::Error
            } else {
                log::LevelFilter::Info
            };
            env_logger::builder().filter_level(level).init();
            if let Err(err) = run(command) {
                eprintln!("Error: {:?}", err);
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
