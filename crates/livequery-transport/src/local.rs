//! Local-only, newline-delimited JSON transport in a private Unix directory.
use crate::{Request, Response};
use livequery_core::{Error, Limits};
use std::{
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// A bounded Unix listener serving a caller-provided request handler.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    limits: Limits,
    identity: (u64, u64),
}
impl Server {
    /// Creates a socket inside an owner-only directory; existing sockets are never replaced.
    /// # Errors
    /// Rejects symlinks, wrong owner, nonprivate directories, existing paths and I/O failures.
    pub fn bind(path: impl AsRef<Path>, limits: Limits) -> Result<Self, Error> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or_else(|| Error::Invalid("socket needs a private parent directory".into()))?;
        match fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        let path = check_directory(parent)?.join(
            path.file_name()
                .ok_or_else(|| Error::Invalid("socket filename".into()))?,
        );
        let path = path.as_path();
        let listener = UnixListener::bind(path)?;
        let metadata = fs::symlink_metadata(path)?;
        let server = Self {
            listener,
            path: path.into(),
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
    /// I/O deadlines are per client. The trusted handler must return promptly;
    /// use livequery-agent for published-only, bounded query dispatch.
    pub fn serve(
        &self,
        stop: &AtomicBool,
        handle: impl Fn(Request) -> Result<Response, Error>,
    ) -> Result<(), Error> {
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
            clients.retain_mut(|client| {
                !stop.load(Ordering::Relaxed) && client.advance(self, &handle)
            });
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
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
    let dir = check_directory(&dir)?;
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
    let path = check_directory(
        path.parent()
            .ok_or_else(|| Error::Invalid("socket parent".into()))?,
    )?
    .join(
        path.file_name()
            .ok_or_else(|| Error::Invalid("socket filename".into()))?,
    );
    let meta = fs::symlink_metadata(&path)?;
    if !meta.file_type().is_socket()
        || meta.uid() != rustix::process::getuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err(Error::Invalid(
            "socket must be a private socket owned by this user".into(),
        ));
    }
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
    socket.connect_timeout(&socket2::SockAddr::unix(&path)?, IO_TIMEOUT)?;
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
fn check_directory(path: &Path) -> Result<PathBuf, Error> {
    // Reject a final symlink even if it currently points to a private directory.
    if !fs::symlink_metadata(path)?.is_dir() {
        return Err(Error::Invalid(
            "socket directory must be a real directory".into(),
        ));
    }
    let canonical = fs::canonicalize(path)?;
    let uid = rustix::process::getuid().as_raw();
    // Validate from the root down so an untrusted ancestor cannot replace an
    // already-validated child. Callers must reuse this exact resolved path.
    let ancestors: Vec<_> = canonical.ancestors().collect();
    for ancestor in ancestors.into_iter().rev() {
        let meta = fs::symlink_metadata(ancestor)?;
        let private_leaf = ancestor == canonical;
        let trusted_owner = meta.uid() == 0 || meta.uid() == uid;
        let sticky_root = meta.uid() == 0 && meta.mode() & 0o1000 != 0;
        if !meta.is_dir()
            || !trusted_owner
            || (private_leaf && (meta.uid() != uid || meta.mode() & 0o077 != 0))
            || (!private_leaf && meta.mode() & 0o022 != 0 && !sticky_root)
        {
            return Err(Error::Invalid(
                "socket directory chain must be private and protected from untrusted replacement"
                    .into(),
            ));
        }
    }
    Ok(canonical)
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
            return Err(
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "incomplete frame").into(),
            );
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
    let code = error.code();
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
        let mut bytes = livequery_core::json_bytes(&response, max).unwrap_or_else(|_| {
            br#"{"status":"error","code":"limit","message":"response bytes exceeded"}"#.to_vec()
        });
        bytes.push(b'\n');
        self.phase = Phase::Writing { bytes, offset: 0 };
        self.deadline = Instant::now() + IO_TIMEOUT;
        true
    }
    fn advance(
        &mut self,
        server: &Server,
        handle: &impl Fn(Request) -> Result<Response, Error>,
    ) -> bool {
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
                            .and_then(handle)
                        {
                            Ok(response) => response,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validated_directory_is_reused_after_original_alias_changes() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("lq-resolved-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let safe = root.join("safe");
        let other = root.join("other");
        for parent in [&safe, &other] {
            fs::DirBuilder::new().mode(0o700).create(parent).unwrap();
            fs::DirBuilder::new()
                .mode(0o700)
                .create(parent.join("private"))
                .unwrap();
        }
        let alias = root.join("alias");
        symlink(&safe, &alias).unwrap();
        let original = alias.join("private");
        let checked = check_directory(&original).unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&other, &alias).unwrap();
        fs::set_permissions(other.join("private"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(check_directory(&original).is_err());
        assert_eq!(checked, fs::canonicalize(safe.join("private")).unwrap());
        let listener = UnixListener::bind(checked.join("test.sock")).unwrap();
        assert!(!other.join("private/test.sock").exists());
        drop(listener);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_stops_between_ready_clients() {
        use std::sync::atomic::AtomicUsize;
        let root = std::env::temp_dir().join(format!("lqstop-{}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let path = root.join("a.sock");
        let server = Server::bind(&path, Limits::default()).unwrap();
        let mut peers = Vec::new();
        for _ in 0..16 {
            let mut peer = UnixStream::connect(&path).unwrap();
            peer.write_all(b"{\"command\":\"schema\"}\n").unwrap();
            peers.push(peer);
        }
        let stop = AtomicBool::new(false);
        let calls = AtomicUsize::new(0);
        server
            .serve(&stop, |_| {
                calls.fetch_add(1, Ordering::Relaxed);
                stop.store(true, Ordering::Relaxed);
                Ok(Response::Ok {
                    data: serde_json::Value::Null,
                })
            })
            .unwrap();
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "shutdown must not dispatch queued queries"
        );
        drop(peers);
        drop(server);
        assert!(!path.exists());
        fs::remove_dir(root).unwrap();
    }
}
