use std::io;
use std::os::fd::AsFd;
use std::os::fd::BorrowedFd;
use std::sync::Arc;

use nix::errno::Errno;

use crate::dev_fuse::DevFuse;
#[cfg(target_os = "macos")]
use crate::ll::fuse_abi::fuse_in_header;
use crate::passthrough::BackingId;

/// A raw communication channel to the FUSE kernel driver
#[derive(Debug, Clone)]
pub(crate) struct Channel(
    Arc<DevFuse>,
    #[cfg(target_os = "linux")] Option<Arc<ChannelStop>>,
);

/// Level-triggered wakeup: never drain the socket, so every receiver wakes.
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct ChannelStop {
    reader: std::os::unix::net::UnixStream,
    writer: std::os::unix::net::UnixStream,
}

#[cfg(target_os = "linux")]
impl ChannelStop {
    pub(crate) fn stop(&self) -> io::Result<()> {
        use std::io::Write;
        match (&self.writer).write(&[1]) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(error) => Err(error),
        }
    }
}

impl AsFd for Channel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Channel {
    /// Create a new communication channel to the kernel driver by mounting the
    /// given path. The kernel driver will delegate filesystem operations of
    /// the given path to the channel.
    pub(crate) fn new(device: Arc<DevFuse>) -> Self {
        Self(
            device,
            #[cfg(target_os = "linux")]
            None,
        )
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn enable_stop(&mut self) -> io::Result<Arc<ChannelStop>> {
        use std::os::fd::AsRawFd;
        let (reader, writer) = std::os::unix::net::UnixStream::pair()?;
        writer.set_nonblocking(true)?;
        // SAFETY: the owned device remains alive throughout both fcntl calls.
        let flags = unsafe { libc::fcntl(self.0.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(self.0.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                < 0
        {
            return Err(io::Error::last_os_error());
        }
        let stop = Arc::new(ChannelStop { reader, writer });
        self.1 = Some(stop.clone());
        Ok(stop)
    }

    /// Receives data up to the capacity of the given buffer (can block).
    #[cfg(not(target_os = "macos"))]
    fn receive(&self, buffer: &mut [u8]) -> nix::Result<usize> {
        #[cfg(target_os = "linux")]
        if let Some(stop) = &self.1 {
            use std::os::fd::AsRawFd;
            let mut fds = [
                libc::pollfd {
                    fd: self.0.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: stop.reader.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            // SAFETY: both descriptors and the two-element array remain live.
            let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if result < 0 {
                return Err(Errno::last());
            }
            if fds[1].revents != 0 {
                return Err(Errno::ENODEV);
            }
        }
        nix::unistd::read(&self.0, buffer)
    }

    /// Receives exactly one framed request from FUSE-T's stream socket.
    #[cfg(target_os = "macos")]
    fn receive(&self, buffer: &mut [u8]) -> nix::Result<usize> {
        let header_len = size_of::<fuse_in_header>();
        if buffer.len() < header_len {
            return Err(Errno::EINVAL);
        }

        let mut total = self.read_exact_framed(buffer, 0, header_len)?;
        if total == 0 {
            return Ok(0);
        }

        let packet_len =
            u32::from_ne_bytes(buffer[..4].try_into().expect("header length bytes")) as usize;
        if packet_len < header_len || packet_len > buffer.len() {
            return Err(Errno::EIO);
        }

        total = self.read_exact_framed(buffer, total, packet_len)?;
        Ok(total)
    }

    #[cfg(target_os = "macos")]
    fn read_exact_framed(
        &self,
        buffer: &mut [u8],
        mut total: usize,
        expected: usize,
    ) -> nix::Result<usize> {
        while total < expected {
            match nix::unistd::read(&self.0, &mut buffer[total..expected]) {
                Ok(0) if total == 0 => return Ok(0),
                Ok(0) => return Err(Errno::EIO),
                Ok(size) => total += size,
                Err(Errno::ENOENT | Errno::EINTR | Errno::EAGAIN) => continue,
                Err(err) => return Err(err),
            }
        }
        Ok(total)
    }

    /// Receives data up to the capacity of the given buffer (can block),
    /// retrying on errors that are safe to retry (ENOENT, EINTR, EAGAIN).
    ///
    /// - ENOENT: Operation interrupted. According to FUSE, this is safe to retry.
    /// - EINTR: Interrupted system call, retry.
    /// - EAGAIN: Explicitly instructed to try again.
    pub(crate) fn receive_retrying(&self, buffer: &mut [u8]) -> nix::Result<usize> {
        loop {
            match self.receive(buffer) {
                Ok(size) => return Ok(size),
                Err(Errno::ENOENT | Errno::EINTR | Errno::EAGAIN) => continue,
                Err(err) => return Err(err),
            }
        }
    }

    /// Returns a sender object for this channel. The sender object can be
    /// used to send to the channel. Multiple sender objects can be used
    /// and they can safely be sent to other threads.
    pub(crate) fn sender(&self) -> ChannelSender {
        // Since write/writev syscalls are threadsafe, we can simply create
        // a sender by using the same file and use it in other threads.
        ChannelSender(self.0.clone())
    }

    /// Clone the FUSE device fd using FUSE_DEV_IOC_CLONE ioctl.
    ///
    /// This creates a new fd that can read FUSE requests independently,
    /// enabling true parallel request processing. The kernel distributes
    /// requests across all cloned fds.
    ///
    /// Requires Linux 4.5+. Returns an error on older kernels or non-Linux.
    #[cfg(target_os = "linux")]
    pub(crate) fn clone_fd(&self) -> io::Result<Channel> {
        use std::os::fd::AsRawFd;

        let new_dev = DevFuse::open()?;

        let mut source_fd = self.0.as_raw_fd() as u32;
        // SAFETY: fuse_dev_ioc_clone is a valid ioctl for /dev/fuse
        unsafe {
            crate::ll::ioctl::fuse_dev_ioc_clone(new_dev.as_raw_fd(), &mut source_fd)?;
        }

        let mut channel = Channel::new(Arc::new(new_dev));
        if let Some(stop) = &self.1 {
            // Cloned FUSE descriptors have independent status flags.
            // SAFETY: channel owns this live device descriptor.
            let flags = unsafe { libc::fcntl(channel.0.as_raw_fd(), libc::F_GETFL) };
            if flags < 0
                || unsafe {
                    libc::fcntl(
                        channel.0.as_raw_fd(),
                        libc::F_SETFL,
                        flags | libc::O_NONBLOCK,
                    )
                } < 0
            {
                return Err(io::Error::last_os_error());
            }
            channel.1 = Some(stop.clone());
        }
        Ok(channel)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ChannelSender(Arc<DevFuse>);

impl ChannelSender {
    pub(crate) fn send(&self, bufs: &[io::IoSlice<'_>]) -> io::Result<()> {
        let rc = nix::sys::uio::writev(&self.0, bufs)?;
        // writev is atomic, so do not need to check how many bytes are written.
        // libfuse does not do it either
        // https://github.com/libfuse/libfuse/blob/6278995cca991978abd25ebb2c20ebd3fc9e8a13/lib/fuse_lowlevel.c#L267
        debug_assert_eq!(bufs.iter().map(|b| b.len()).sum::<usize>(), rc);
        Ok(())
    }

    pub(crate) fn open_backing(&self, fd: BorrowedFd<'_>) -> std::io::Result<BackingId> {
        BackingId::create(&self.0, fd)
    }
}

#[cfg(all(test, target_os = "linux"))]
mod stop_tests {
    use super::*;

    #[test]
    fn stopping_wakes_all_receivers_and_is_idempotent() {
        use std::os::fd::OwnedFd;
        let (device, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let file = std::fs::File::from(OwnedFd::from(device));
        let mut channel = Channel::new(Arc::new(DevFuse(file)));
        let stop = channel.enable_stop().unwrap();
        let (done, completion) = std::sync::mpsc::channel();
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let channel = channel.clone();
                let done = done.clone();
                std::thread::spawn(move || {
                    done.send(channel.receive_retrying(&mut [0; 32])).unwrap();
                })
            })
            .collect();
        stop.stop().unwrap();
        stop.stop().unwrap();
        for _ in 0..4 {
            assert_eq!(
                completion
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap(),
                Err(Errno::ENODEV)
            );
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(channel.receive_retrying(&mut [0; 32]), Err(Errno::ENODEV));
    }
}
