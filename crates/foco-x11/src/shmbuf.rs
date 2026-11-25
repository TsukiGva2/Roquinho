//! A memfd-backed shared-memory buffer, mapped locally with mmap and handed to the X server
//! with MIT-SHM `AttachFd`.

use std::os::fd::OwnedFd;

use rustix::fs::{ftruncate, memfd_create, MemfdFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};

pub struct ShmBuf {
    ptr: *mut u8,
    len: usize,
}

impl ShmBuf {
    /// Creates the segment; returns the mapping and the fd to give to the server.
    pub fn new(len: usize) -> Result<(ShmBuf, OwnedFd), String> {
        let fd = memfd_create("foco-max-shm", MemfdFlags::CLOEXEC)
            .map_err(|e| format!("memfd_create: {e}"))?;
        ftruncate(&fd, len as u64).map_err(|e| format!("ftruncate: {e}"))?;
        // SAFETY: fresh anonymous mapping of a memfd we own, length checked by ftruncate; the
        // mapping outlives all slices handed out (freed in Drop).
        let ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &fd,
                0,
            )
        }
        .map_err(|e| format!("mmap: {e}"))?;
        Ok((
            ShmBuf {
                ptr: ptr.cast(),
                len,
            },
            fd,
        ))
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr/len describe a live RW mapping owned by self; &mut self gives exclusivity
        // on our side (the server only reads, and the caller waits for ShmCompletion).
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for ShmBuf {
    fn drop(&mut self) {
        // SAFETY: unmapping the region mapped in `new`.
        unsafe {
            let _ = munmap(self.ptr.cast(), self.len);
        }
    }
}
