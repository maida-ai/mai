//! One trusted-local engine owns an authority. The OS lock, never PID metadata,
//! is the ownership decision. Receive and landing are deliberately unavailable.
use crate::{authority::Authority, native_path::NativePath};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::{Command, Stdio};

const PROTOCOL_VERSION: u32 = 1;
const MAX_FRAME: usize = 4096;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const START_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub protocol_version: u32,
    pub pid: u32,
    pub authority: NativePath,
    pub state: String,
    pub registration_available: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol_version: u32,
    action: Action,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    Status,
    Stop,
    Ack,
}

fn local_dir(root: &Path) -> Result<PathBuf> {
    ensure!(
        fs::symlink_metadata(root)?.is_dir(),
        "authority must be a real directory, not a symlink"
    );
    let root = root
        .canonicalize()
        .context("authority does not exist; run mai init first")?;
    let local = root.join(".mai");
    ensure!(
        fs::symlink_metadata(&local)?.is_dir(),
        "authority .mai must be a real directory"
    );
    Ok(local)
}
fn regular_or_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => ensure!(
            meta.is_file(),
            "{} must be a regular file, not a symlink",
            path.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn lock_file(local: &Path) -> Result<File> {
    let path = local.join("engine.lock");
    regular_or_absent(&path)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .context("open persistent engine ownership lock")
}

struct Ownership(File);
impl Drop for Ownership {
    fn drop(&mut self) {
        // Explicit unlock also releases a flock inherited briefly across a
        // concurrent fork before CLOEXEC closes that child's descriptor.
        let _ = FileExt::unlock(&self.0);
    }
}

/// Serve until explicit `mai stop`. OS termination releases ownership; a later
/// owner removes stale endpoints only after acquiring this same persistent lock.
pub fn serve(root: &Path) -> Result<()> {
    let local = local_dir(root)?;
    let lock = lock_file(&local)?;
    lock.try_lock_exclusive()
        .context("authority already owned by a live engine; use mai status or mai stop")?;
    let ownership = Ownership(lock);
    let mut authority = Authority::open(root)?;
    authority
        .reconcile()
        .context("authority startup reconciliation failed")?;
    regular_or_absent(&local.join("engine.pid"))?;
    let listener = transport::Listener::bind(&local)?;
    let _cleanup = Cleanup {
        local: local.clone(),
    };
    fs::write(
        local.join("engine.pid"),
        format!("{}\n", std::process::id()),
    )?;
    let mut status = Status {
        protocol_version: PROTOCOL_VERSION,
        pid: std::process::id(),
        authority: NativePath::from_path(&root.canonicalize()?)?,
        state: "ready".into(),
        registration_available: false,
    };
    loop {
        if let Some(mut stream) = listener.accept()? {
            let deadline = Instant::now() + IO_TIMEOUT;
            let result = (|| -> Result<bool> {
                let request: Request = read_frame(&mut stream, deadline)?;
                ensure!(
                    request.protocol_version == PROTOCOL_VERSION,
                    "unsupported IPC version"
                );
                let stop = match request.action {
                    Action::Status => false,
                    Action::Stop => true,
                    Action::Ack => bail!("unexpected acknowledgment"),
                };
                if stop {
                    status.state = "stopping".into();
                }
                let written = write_frame(&mut stream, &status, deadline);
                // Once a valid stop is accepted, a disconnected requester must
                // not leave a live engine advertising a false stopping state.
                if stop {
                    if written.is_ok() {
                        let _ = read_frame::<Request>(&mut stream, deadline);
                    }
                    return Ok(true);
                }
                written?;
                // A bounded acknowledgment guarantees Windows clients consumed
                // the response without an unbounded FlushFileBuffers call.
                if let Ok(ack) = read_frame::<Request>(&mut stream, deadline) {
                    ensure!(
                        ack.protocol_version == PROTOCOL_VERSION
                            && matches!(ack.action, Action::Ack),
                        "invalid acknowledgment"
                    );
                }
                Ok(stop)
            })();
            match result {
                Ok(true) => break,
                Ok(false) => {}
                Err(error) => eprintln!("local IPC request rejected: {error:#}"),
            }
        } else {
            thread::sleep(POLL);
        }
    }
    drop(listener);
    // Cleanup runs while lock is held, and lock file is never removed.
    drop(_cleanup);
    drop(ownership);
    Ok(())
}
struct Cleanup {
    local: PathBuf,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.local.join("engine.pid"));
        #[cfg(unix)]
        let _ = fs::remove_file(self.local.join("engine.sock"));
    }
}

fn exchange(local: &Path, action: Action, timeout: Duration) -> Result<Status> {
    let deadline = Instant::now() + timeout;
    let mut stream = transport::connect(local, deadline)?;
    write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action,
        },
        deadline,
    )?;
    let response: Status = read_frame(&mut stream, deadline)?;
    ensure!(
        response.protocol_version == PROTOCOL_VERSION,
        "unsupported engine IPC version"
    );
    ensure!(
        response.authority.as_path() == local.parent().unwrap(),
        "engine responded for another authority"
    );
    write_frame(
        &mut stream,
        &Request {
            protocol_version: PROTOCOL_VERSION,
            action: Action::Ack,
        },
        deadline,
    )?;
    Ok(response)
}

pub fn status(root: &Path, autostart: bool) -> Result<Status> {
    let local = local_dir(root)?;
    if let Ok(status) = exchange(&local, Action::Status, Duration::from_millis(200)) {
        return Ok(status);
    }
    if !autostart {
        bail!(
            "engine unavailable; run mai serve {} --foreground or mai status {} to autostart",
            root.display(),
            root.display()
        );
    }
    let deadline = Instant::now() + START_TIMEOUT;
    let log_path = local.join("engine.log");
    regular_or_absent(&log_path)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut child = spawn_background(&root.canonicalize()?, log)?;
    loop {
        if let Ok(status) = exchange(&local, Action::Status, Duration::from_millis(200)) {
            return Ok(status);
        }
        if let Some(exit) = child.try_wait()? {
            // Another racing starter may hold ownership and still reconcile.
            let lock = lock_file(&local)?;
            if lock.try_lock_exclusive().is_ok() {
                FileExt::unlock(&lock)?;
                bail!(
                    "engine exited ({exit}) before readiness; inspect {} and run mai serve --foreground",
                    log_path.display()
                );
            }
        }
        ensure!(
            Instant::now() < deadline,
            "engine readiness timed out; inspect {} and run mai serve --foreground (a starting/live owner is never killed)",
            log_path.display()
        );
        thread::sleep(POLL);
    }
}

#[cfg(unix)]
fn spawn_background(root: &Path, log: File) -> Result<std::process::Child> {
    Command::new(std::env::current_exe()?)
        .arg("serve")
        .arg(root)
        .arg("--foreground")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .context("failed to spawn engine")
}

#[cfg(windows)]
use background::spawn_background;

#[cfg(windows)]
#[path = "engine/windows_process.rs"]
mod background;

pub fn stop(root: &Path) -> Result<Status> {
    exchange(&local_dir(root)?, Action::Stop, IO_TIMEOUT)
        .context("engine unavailable; stop never autostarts an engine")
}

fn read_frame<T: serde::de::DeserializeOwned>(
    stream: &mut impl Read,
    deadline: Instant,
) -> Result<T> {
    let mut bytes = Vec::new();
    loop {
        ensure!(Instant::now() < deadline, "IPC read timed out");
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => bail!("IPC peer closed before a complete frame"),
            Ok(_) if byte[0] == b'\n' => {
                return serde_json::from_slice(&bytes).context("invalid IPC frame");
            }
            Ok(_) => {
                ensure!(
                    bytes.len() < MAX_FRAME,
                    "IPC frame exceeds {MAX_FRAME} bytes"
                );
                bytes.push(byte[0]);
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::Interrupted =>
            {
                thread::sleep(POLL)
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn write_frame(stream: &mut impl Write, value: &impl Serialize, deadline: Instant) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "IPC frame too large");
    bytes.push(b'\n');
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        ensure!(Instant::now() < deadline, "IPC write timed out");
        match stream.write(remaining) {
            Ok(0) => bail!("IPC peer stopped accepting data"),
            Ok(n) => remaining = &remaining[n..],
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::Interrupted =>
            {
                thread::sleep(POLL)
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    };
    pub struct Listener(UnixListener);
    impl Listener {
        pub fn bind(local: &Path) -> Result<Self> {
            let path = local.join("engine.sock");
            if let Ok(meta) = fs::symlink_metadata(&path) {
                ensure!(
                    meta.file_type().is_socket(),
                    "refusing to replace non-socket engine.sock"
                );
                fs::remove_file(&path)?;
            }
            let listener = UnixListener::bind(&path)
                .context("bind engine socket (authority path may exceed OS socket path limit)")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            Ok(Self(listener))
        }
        pub fn accept(&self) -> Result<Option<UnixStream>> {
            match self.0.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true)?;
                    Ok(Some(stream))
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
    }
    pub fn connect(local: &Path, deadline: Instant) -> Result<UnixStream> {
        use std::os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::ffi::OsStrExt,
        };
        let path = local.join("engine.sock");
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: zero is a valid initial sockaddr_un; fields are filled below.
        let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
        ensure!(
            bytes.len() < address.sun_path.len() && !bytes.contains(&0),
            "authority path exceeds Unix socket path limit"
        );
        address.sun_family = libc::AF_UNIX as libc::sa_family_t;
        for (out, byte) in address.sun_path.iter_mut().zip(bytes) {
            *out = *byte as libc::c_char;
        }
        let length = std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
        #[cfg(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd",
            target_os = "dragonfly"
        ))]
        {
            address.sun_len = length as u8;
        }
        loop {
            ensure!(Instant::now() < deadline, "IPC connect timed out");
            // SAFETY: socket has no pointer arguments and returns a new owned fd.
            let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
            ensure!(
                raw >= 0,
                "create Unix socket: {}",
                io::Error::last_os_error()
            );
            // SAFETY: raw is a fresh successful socket and ownership transfers once.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: fcntl operates on a valid owned socket. Set nonblocking
            // before connect so a full listener backlog cannot hang the CLI.
            ensure!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } >= 0,
                "set socket nonblocking: {}",
                io::Error::last_os_error()
            );
            ensure!(
                unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } >= 0,
                "set socket close-on-exec: {}",
                io::Error::last_os_error()
            );
            // SAFETY: initialized sockaddr_un contains a NUL-terminated path;
            // length describes its initialized family/path bytes on this OS.
            let connected = unsafe {
                libc::connect(
                    fd.as_raw_fd(),
                    (&address as *const libc::sockaddr_un).cast(),
                    length as libc::socklen_t,
                )
            };
            if connected == 0 {
                return Ok(UnixStream::from(fd));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINPROGRESS) {
                loop {
                    ensure!(Instant::now() < deadline, "IPC connect timed out");
                    let mut poll = libc::pollfd {
                        fd: fd.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    let millis = deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .min(i32::MAX as u128) as i32;
                    // SAFETY: one initialized pollfd lives throughout the bounded call.
                    let result = unsafe { libc::poll(&mut poll, 1, millis) };
                    if result < 0 {
                        let error = io::Error::last_os_error();
                        if error.kind() == io::ErrorKind::Interrupted {
                            continue;
                        }
                        return Err(error.into());
                    }
                    ensure!(result > 0, "IPC connect timed out");
                    let mut error: libc::c_int = 0;
                    let mut length = std::mem::size_of_val(&error) as libc::socklen_t;
                    // SAFETY: correctly sized writable error/length pointers and live fd.
                    ensure!(
                        unsafe {
                            libc::getsockopt(
                                fd.as_raw_fd(),
                                libc::SOL_SOCKET,
                                libc::SO_ERROR,
                                (&mut error as *mut libc::c_int).cast(),
                                &mut length,
                            )
                        } == 0,
                        "read socket connection result: {}",
                        io::Error::last_os_error()
                    );
                    ensure!(
                        error == 0,
                        "connect Unix socket: {}",
                        io::Error::from_raw_os_error(error)
                    );
                    return Ok(UnixStream::from(fd));
                }
            }
            if error.kind() != io::ErrorKind::WouldBlock
                && error.kind() != io::ErrorKind::Interrupted
            {
                return Err(error.into());
            }
            // A failed connect's state is unspecified: close it and retry on a
            // fresh socket, always against the original bounded deadline.
            drop(fd);
            thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
        }
    }
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::{
        cell::Cell,
        hash::{Hash, Hasher},
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle},
        },
    };
    use windows_sys::Win32::{
        Foundation::{
            ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
            ERROR_PIPE_LISTENING, INVALID_HANDLE_VALUE,
        },
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, ReadFile},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_NOWAIT,
            PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            SetNamedPipeHandleState,
        },
    };
    fn name(local: &Path) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        local
            .as_os_str()
            .encode_wide()
            .collect::<Vec<_>>()
            .hash(&mut hash);
        format!(r"\\.\pipe\mai-{:016x}", hash.finish())
    }
    fn create(local: &Path) -> Result<File> {
        let name: Vec<u16> = name(local).encode_utf16().chain(Some(0)).collect();
        // SAFETY: name is NUL terminated and lives for the call; no security
        // descriptor or overlapped pointer is supplied. File owns the handle.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_FRAME as u32 + 1,
                MAX_FRAME as u32 + 1,
                0,
                std::ptr::null(),
            )
        };
        ensure!(
            handle != INVALID_HANDLE_VALUE,
            "create local named pipe: {}",
            io::Error::last_os_error()
        );
        Ok(unsafe { File::from_raw_handle(handle) })
    }
    pub struct Listener {
        file: File,
        connected: Cell<bool>,
    }
    impl Listener {
        pub fn bind(local: &Path) -> Result<Self> {
            Ok(Self {
                file: create(local)?,
                connected: Cell::new(false),
            })
        }
        pub fn accept(&self) -> Result<Option<Stream>> {
            if self.connected.replace(false) {
                // SAFETY: listener retains the original valid server handle.
                // The previous response has been acknowledged or timed out.
                // Reuse this instance even while a previous client closes its
                // handle; creating another FIRST_PIPE_INSTANCE would race it.
                unsafe {
                    DisconnectNamedPipe(self.file.as_raw_handle());
                }
            }
            // SAFETY: File owns a valid named pipe; nonblocking mode needs no OVERLAPPED.
            let connected =
                unsafe { ConnectNamedPipe(self.file.as_raw_handle(), std::ptr::null_mut()) };
            if connected != 0 {
                return Ok(None);
            } // PIPE_NOWAIT transitioned to listening.
            let error = io::Error::last_os_error();
            match error.raw_os_error().map(|code| code as u32) {
                Some(ERROR_PIPE_CONNECTED) => {
                    self.connected.set(true);
                    Ok(Some(Stream(self.file.try_clone()?)))
                }
                Some(ERROR_PIPE_LISTENING) => Ok(None),
                Some(ERROR_NO_DATA) => {
                    // SAFETY: a client closed before acceptance; reset the
                    // owned instance so the next client can connect.
                    unsafe {
                        DisconnectNamedPipe(self.file.as_raw_handle());
                    }
                    Ok(None)
                }
                _ => Err(error.into()),
            }
        }
    }
    pub struct Stream(File);
    fn map_nonblocking<T>(result: io::Result<T>) -> io::Result<T> {
        result.map_err(|error| {
            if error.raw_os_error() == Some(ERROR_NO_DATA as i32) {
                io::Error::from(io::ErrorKind::WouldBlock)
            } else {
                error
            }
        })
    }
    impl Read for Stream {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            let mut count = 0;
            // SAFETY: valid synchronous pipe handle and writable buffer/count;
            // PIPE_NOWAIT bounds this call and no OVERLAPPED is required.
            // Do not use File::read: Rust maps ERROR_NO_DATA to BrokenPipe and
            // then EOF, losing the distinction between idle and disconnected.
            let success = unsafe {
                ReadFile(
                    self.0.as_raw_handle(),
                    bytes.as_mut_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    &mut count,
                    std::ptr::null_mut(),
                )
            };
            if success != 0 {
                return Ok(count as usize);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error().map(|code| code as u32) {
                Some(ERROR_NO_DATA) => Err(io::ErrorKind::WouldBlock.into()),
                Some(ERROR_BROKEN_PIPE) => Ok(0),
                _ => Err(error),
            }
        }
    }
    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match map_nonblocking(self.0.write(bytes)) {
                Ok(0) if !bytes.is_empty() => Err(io::ErrorKind::WouldBlock.into()),
                result => result,
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    pub fn connect(local: &Path, deadline: Instant) -> Result<Stream> {
        let file = loop {
            ensure!(Instant::now() < deadline, "IPC connect timed out");
            match OpenOptions::new().read(true).write(true).open(name(local)) {
                Ok(file) => break file,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    // The previous acknowledged client may still be closing.
                    // Respect the caller's original deadline while the single
                    // server instance disconnects and becomes available again.
                    thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
                }
                Err(error) => return Err(error.into()),
            }
        };
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        // SAFETY: valid pipe handle and mode pointer; optional output pointers absent.
        ensure!(
            unsafe {
                SetNamedPipeHandleState(
                    file.as_raw_handle(),
                    &mode,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            } != 0,
            "set nonblocking pipe mode: {}",
            io::Error::last_os_error()
        );
        Ok(Stream(file))
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        fn accept(listener: &Listener) -> Stream {
            let deadline = Instant::now() + IO_TIMEOUT;
            loop {
                if let Some(stream) = listener.accept().unwrap() {
                    return stream;
                }
                assert!(Instant::now() < deadline, "named pipe was not accepted");
                thread::sleep(POLL);
            }
        }
        #[test]
        fn idle_pipe_read_is_would_block_and_instance_survives_old_client_handle() {
            let temp = tempfile::tempdir().unwrap();
            let listener = Listener::bind(temp.path()).unwrap();
            let mut client = connect(temp.path(), Instant::now() + IO_TIMEOUT).unwrap();
            let mut server = accept(&listener);
            let start = Instant::now();
            assert!(connect(temp.path(), start + Duration::from_millis(40)).is_err());
            assert!(start.elapsed() < Duration::from_secs(1));
            let mut byte = [0];
            assert_eq!(
                client.read(&mut byte).unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            assert_eq!(
                server.read(&mut byte).unwrap_err().kind(),
                io::ErrorKind::WouldBlock
            );
            client.write_all(b"x").unwrap();
            assert_eq!(server.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'x']);
            server.write_all(b"y").unwrap();
            assert_eq!(client.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'y']);
            drop(server);
            // Keep the old client alive deliberately. The listener resets and
            // reuses its instance instead of racing FIRST_PIPE_INSTANCE.
            assert!(listener.accept().unwrap().is_none());
            let mut next = connect(temp.path(), Instant::now() + IO_TIMEOUT).unwrap();
            let mut server = accept(&listener);
            next.write_all(b"z").unwrap();
            assert_eq!(server.read(&mut byte).unwrap(), 1);
            assert_eq!(byte, [b'z']);
            drop(next);
            assert_eq!(server.read(&mut byte).unwrap(), 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn connect_deadline_survives_a_full_listener_backlog() {
        use std::os::{fd::AsRawFd, unix::net::UnixListener};
        let temp = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(temp.path().join("engine.sock")).unwrap();
        // SAFETY: valid listening socket; reduce backlog to saturate deterministically.
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 1) }, 0);
        let mut clients = Vec::new();
        for _ in 0..32 {
            match transport::connect(temp.path(), Instant::now() + Duration::from_millis(50)) {
                Ok(client) => clients.push(client),
                Err(_) => break,
            }
        }
        assert!(
            !clients.is_empty() && clients.len() < 32,
            "failed to saturate listener backlog"
        );
        let start = Instant::now();
        assert!(transport::connect(temp.path(), start + Duration::from_millis(150)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn frames_reject_bad_versions_fields_truncation_and_oversize() {
        let deadline = Instant::now() + Duration::from_secs(1);
        assert!(
            read_frame::<Request>(
                &mut b"{\"protocol_version\":1,\"action\":\"status\"}\n".as_slice(),
                deadline
            )
            .is_ok()
        );
        for payload in [
            b"{}\n".as_slice(),
            b"{\"protocol_version\":1,\"action\":\"receive\"}\n",
            b"{\"protocol_version\":1,\"action\":\"status\",\"extra\":1}\n",
            b"{",
        ] {
            assert!(read_frame::<Request>(&mut &*payload, deadline).is_err());
        }
        assert!(
            read_frame::<Request>(&mut vec![b'x'; MAX_FRAME + 1].as_slice(), deadline).is_err()
        );
    }
}
