//! Local-only, newline-delimited JSON transport in a private Unix directory.
use crate::{Error, Limits, Query, Registry};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Version-one read-only request; one request is served per connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Request {
    /// Discover source schemas.
    Schema,
    /// Execute one bounded query.
    Query {
        /// SELECT statement.
        sql: String,
    },
}
/// Machine-readable success or diagnostic response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    /// JSON schema map or query result.
    Ok {
        /// Response payload.
        data: serde_json::Value,
    },
    /// Stable error category and readable detail.
    Error {
        /// Error category.
        code: String,
        /// Diagnostic detail.
        message: String,
    },
}
/// One bounded local agent, run on a caller-owned thread.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    registry: Registry,
    limits: Limits,
    identity: (u64, u64),
}
impl Server {
    /// Creates a socket inside an owner-only directory; existing sockets are never replaced.
    /// # Errors
    /// Rejects symlinks, wrong owner, nonprivate directories, existing paths and I/O failures.
    pub fn bind(path: impl AsRef<Path>, registry: Registry, limits: Limits) -> Result<Self, Error> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or_else(|| Error::Invalid("socket needs a private parent directory".into()))?;
        match fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        check_directory(parent)?;
        let listener = UnixListener::bind(path)?;
        let metadata = fs::symlink_metadata(path)?;
        let server = Self {
            listener,
            path: path.into(),
            registry,
            limits,
            identity: (metadata.dev(), metadata.ino()),
        };
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        server.listener.set_nonblocking(true)?;
        Ok(server)
    }
    /// Serves at most sixteen nonblocking clients on the caller's thread until cancellation.
    /// # Errors
    /// Reports listener errors. Slow, malformed and disconnected clients are isolated.
    /// I/O deadlines are per client; trusted source callbacks must still return promptly.
    pub fn serve(&self, stop: &AtomicBool) -> Result<(), Error> {
        let mut clients = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            for _ in clients.len()..MAX_CLIENTS {
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if stream.set_nonblocking(true).is_ok() {
                            clients.push(Client::new(stream));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            clients.retain_mut(|client| client.advance(self));
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
    fn dispatch(&self, request: Request) -> Result<serde_json::Value, Error> {
        match request {
            Request::Schema => {
                let mut remaining = self.limits.bytes;
                crate::value_with_budget(&self.registry.schemas(), &mut remaining)
            }
            Request::Query { sql } => Ok(serde_json::to_value(
                Query::parse(&sql)?.execute(&self.registry, self.limits)?,
            )?),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| (metadata.dev(), metadata.ino()) == self.identity)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// Default per-user discovery directory, containing only opt-in agent sockets.
pub fn directory() -> PathBuf {
    std::env::temp_dir().join(format!("livequery-{}", rustix::process::getuid().as_raw()))
}
/// Discovers PID-named sockets; presence alone does not establish a live agent.
/// # Errors
/// Rejects insecure discovery directories and filesystem errors.
pub fn discover() -> Result<Vec<PathBuf>, Error> {
    let dir = directory();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    check_directory(&dir)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "sock")
            && path
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.parse::<u32>().is_ok())
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}
/// Sends one request to an explicitly enabled local agent.
/// # Errors
/// Returns security, timeout, malformed-response and I/O errors.
pub fn request(path: impl AsRef<Path>, request: &Request) -> Result<Response, Error> {
    let path = path.as_ref();
    check_directory(
        path.parent()
            .ok_or_else(|| Error::Invalid("socket parent".into()))?,
    )?;
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
    socket.connect_timeout(&socket2::SockAddr::unix(path)?, IO_TIMEOUT)?;
    let descriptor: std::os::fd::OwnedFd = socket.into();
    let mut stream = UnixStream::from(descriptor);
    let mut bytes = serde_json::to_vec(request)?;
    if bytes.len() > 8192 {
        return Err(Error::Limit("request bytes".into()));
    }
    bytes.push(b'\n');
    write_bytes(&mut stream, &bytes)?;
    Ok(serde_json::from_slice(&frame(
        &mut stream,
        Limits::default().bytes + 1024,
    )?)?)
}
fn check_directory(path: &Path) -> Result<(), Error> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::getuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err(Error::Invalid(
            "socket directory must be owned by this user and mode 0700".into(),
        ));
    }
    Ok(())
}
fn frame(stream: &mut UnixStream, max: usize) -> Result<Vec<u8>, Error> {
    stream.set_nonblocking(true)?;
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut result = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Limit("response read deadline".into()));
        }
        let mut buffer = [0; 1024];
        let n = match stream.read(&mut buffer) {
            Ok(n) => n,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if n == 0 {
            return Err(Error::Invalid("incomplete frame".into()));
        }
        let end = buffer[..n].iter().position(|b| *b == b'\n');
        let count = end.unwrap_or(n);
        if result.len() + count > max {
            return Err(Error::Limit("frame bytes".into()));
        }
        result.extend_from_slice(&buffer[..count]);
        if end.is_some() {
            return Ok(result);
        }
    }
}
fn diagnostic(error: Error) -> Response {
    let code = match &error {
        Error::Busy => "busy",
        Error::Limit(_) => "limit",
        Error::NotFound(_) => "not_found",
        Error::Invalid(_) => "invalid",
        Error::Io(_) => "io",
        Error::Serialization(_) => "serialization",
    };
    Response::Error {
        code: code.into(),
        message: error.to_string(),
    }
}
// Fixed caps bound active buffers independently of listener backlog size.
const MAX_CLIENTS: usize = 16;
const REQUEST_BYTES: usize = 8192;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

enum Phase {
    Reading(Vec<u8>),
    Writing { bytes: Vec<u8>, offset: usize },
}
struct Client {
    stream: UnixStream,
    phase: Phase,
    deadline: Instant,
}
impl Client {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            phase: Phase::Reading(Vec::new()),
            deadline: Instant::now() + IO_TIMEOUT,
        }
    }
    fn respond(&mut self, response: Response, max: usize) -> bool {
        let mut bytes = crate::state::json_bytes(&response, max).unwrap_or_else(|_| {
            br#"{"status":"error","code":"limit","message":"response bytes exceeded"}"#.to_vec()
        });
        bytes.push(b'\n');
        self.phase = Phase::Writing { bytes, offset: 0 };
        self.deadline = Instant::now() + IO_TIMEOUT;
        true
    }
    fn advance(&mut self, server: &Server) -> bool {
        if Instant::now() >= self.deadline {
            return false;
        }
        match &mut self.phase {
            Phase::Reading(input) => {
                let mut buffer = [0; 1024];
                match self.stream.read(&mut buffer) {
                    Ok(0) => false,
                    Ok(n) => {
                        let end = buffer[..n].iter().position(|byte| *byte == b'\n');
                        let count = end.unwrap_or(n);
                        if input.len() + count > REQUEST_BYTES {
                            return self.respond(
                                diagnostic(Error::Limit("request bytes".into())),
                                server.limits.bytes,
                            );
                        }
                        input.extend_from_slice(&buffer[..count]);
                        if end.is_none() {
                            return true;
                        }
                        let response = match serde_json::from_slice::<Request>(input)
                            .map_err(Error::from)
                            .and_then(|request| server.dispatch(request))
                        {
                            Ok(data) => Response::Ok { data },
                            Err(error) => diagnostic(error),
                        };
                        self.respond(response, server.limits.bytes)
                    }
                    Err(error) => matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ),
                }
            }
            Phase::Writing { bytes, offset } => match self.stream.write(&bytes[*offset..]) {
                Ok(0) => false,
                Ok(n) => {
                    *offset += n;
                    *offset < bytes.len()
                }
                Err(error) => matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ),
            },
        }
    }
}
fn write_bytes(stream: &mut UnixStream, bytes: &[u8]) -> Result<(), Error> {
    stream.set_nonblocking(true)?;
    let deadline = Instant::now() + IO_TIMEOUT;
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err(Error::Limit("request write deadline".into()));
        }
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(Error::Invalid("peer disconnected".into())),
            Ok(n) => offset += n,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
