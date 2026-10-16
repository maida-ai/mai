use anyhow::{Context, Result, ensure};
use std::{
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::{Child, Command},
};

/// A separate, async-signal-safe watchdog outlives a killed owner. Its lifeline
/// is CLOEXEC; children cannot accidentally keep their owner's claim alive.
pub(super) struct Supervision {
    lifeline: Option<OwnedFd>,
    watcher: libc::pid_t,
    group: Option<libc::pid_t>,
}
impl Supervision {
    pub(super) fn prepare(command: &mut Command) -> Result<Self> {
        let mut descriptors = [0; 2];
        // SAFETY: writable pair, then ownership is immediately transferred.
        ensure!(
            unsafe { libc::pipe(descriptors.as_mut_ptr()) } == 0,
            "watchdog pipe: {}",
            io::Error::last_os_error()
        );
        let read = unsafe { OwnedFd::from_raw_fd(descriptors[0]) };
        let write = unsafe { OwnedFd::from_raw_fd(descriptors[1]) };
        for descriptor in [read.as_raw_fd(), write.as_raw_fd()] {
            ensure!(
                unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } == 0,
                "watchdog CLOEXEC: {}",
                io::Error::last_os_error()
            );
        }
        let reader = read.as_raw_fd();
        let writer = write.as_raw_fd();
        // sysconf is performed before fork; only async-signal-safe operations run
        // in the watcher, even when another runner has active reader threads.
        let max_fd = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) }.max(1024) as i32;
        let watcher = unsafe { libc::fork() };
        ensure!(
            watcher >= 0,
            "fork process watchdog: {}",
            io::Error::last_os_error()
        );
        if watcher == 0 {
            unsafe {
                if libc::setsid() < 0 {
                    libc::_exit(1);
                }
                #[cfg(target_os = "linux")]
                let closed = libc::syscall(libc::SYS_close_range, 3u32, (reader - 1) as u32, 0u32)
                    == 0
                    && libc::syscall(libc::SYS_close_range, (reader + 1) as u32, u32::MAX, 0u32)
                        == 0;
                #[cfg(not(target_os = "linux"))]
                let closed = false;
                if !closed {
                    for descriptor in 3..max_fd {
                        if descriptor != reader {
                            libc::close(descriptor);
                        }
                    }
                }
                let mut group: libc::pid_t = 0;
                let mut filled = 0;
                while filled < std::mem::size_of::<libc::pid_t>() {
                    let count = libc::read(
                        reader,
                        (&mut group as *mut libc::pid_t)
                            .cast::<u8>()
                            .add(filled)
                            .cast(),
                        std::mem::size_of::<libc::pid_t>() - filled,
                    );
                    if count <= 0 {
                        libc::_exit(0);
                    }
                    filled += count as usize;
                }
                let mut byte = 0u8;
                loop {
                    let count = libc::read(reader, (&mut byte as *mut u8).cast(), 1);
                    if count == 0 {
                        break;
                    }
                    if count > 0 {
                        libc::_exit(0);
                    }
                    if count < 0 {
                        continue;
                    }
                }
                libc::kill(-group, libc::SIGKILL);
                libc::_exit(0);
            }
        }
        // Do not launch the candidate until the watchdog has left the owner's
        // terminal/process group. Group termination must not kill supervision.
        let ready = std::time::Instant::now();
        while unsafe { libc::getpgid(watcher) } != watcher {
            ensure!(
                ready.elapsed() < std::time::Duration::from_secs(2),
                "watchdog session initialization timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        drop(read);
        // SAFETY: pre_exec uses only async-signal-safe syscalls. Recording the
        // process group before exec closes the spawn/owner-death race.
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                let group = libc::getpid();
                if libc::write(
                    writer,
                    (&group as *const libc::pid_t).cast(),
                    std::mem::size_of::<libc::pid_t>(),
                ) != std::mem::size_of::<libc::pid_t>() as isize
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(Self {
            lifeline: Some(write),
            watcher,
            group: None,
        })
    }
    pub(super) fn attach(&mut self, child: &mut Child) -> Result<()> {
        self.group = Some(child.id() as libc::pid_t);
        Ok(())
    }
    pub(super) fn quiescent(&self) -> Result<bool> {
        let Some(group) = self.group else {
            return Ok(true);
        };
        #[cfg(target_os = "linux")]
        {
            // A killed orphan can remain as a zombie under a non-reaping
            // container init. Zombies cannot execute or retain slot resources.
            for entry in std::fs::read_dir("/proc")? {
                let entry = entry?;
                if !entry
                    .file_name()
                    .as_encoded_bytes()
                    .iter()
                    .all(u8::is_ascii_digit)
                {
                    continue;
                }
                let stat = match std::fs::read_to_string(entry.path().join("stat")) {
                    Ok(stat) => stat,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let Some((_, fields)) = stat.rsplit_once(')') else {
                    continue;
                };
                let fields: Vec<_> = fields.split_whitespace().collect();
                if fields.len() > 2
                    && fields[2].parse::<libc::pid_t>().ok() == Some(group)
                    && fields[0] != "Z"
                    && fields[0] != "X"
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        #[cfg(not(target_os = "linux"))]
        {
            if unsafe { libc::kill(-group, 0) } == 0 {
                return Ok(false);
            }
            ensure!(
                io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
                "query validator group: {}",
                io::Error::last_os_error()
            );
            Ok(true)
        }
    }
    pub(super) fn terminate(&mut self) -> Result<()> {
        if let Some(group) = self.group {
            let result = unsafe { libc::kill(-group, libc::SIGKILL) };
            if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(io::Error::last_os_error()).context("terminate process group");
            }
        }
        Ok(())
    }
}
impl Drop for Supervision {
    fn drop(&mut self) {
        if self.terminate().is_ok() {
            if let Some(pipe) = &self.lifeline {
                unsafe {
                    libc::write(pipe.as_raw_fd(), b"D".as_ptr().cast(), 1);
                }
            }
        }
        drop(self.lifeline.take());
        unsafe {
            libc::waitpid(self.watcher, std::ptr::null_mut(), 0);
        }
    }
}
