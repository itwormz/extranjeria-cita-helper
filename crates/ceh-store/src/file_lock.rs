//! Normalize fs2 lock contention across Unix and Windows.
use std::{fs::File, io};

pub(crate) fn try_exclusive(file: &File) -> io::Result<()> {
    fs2::FileExt::try_lock_exclusive(file).map_err(|error| {
        // Windows ERROR_LOCK_VIOLATION is not mapped to WouldBlock by std.
        if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() {
            io::Error::new(io::ErrorKind::WouldBlock, error)
        } else {
            error
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contention_is_would_block_and_releasing_the_owner_allows_a_new_lock() {
        let owner = tempfile::NamedTempFile::new().unwrap();
        let other = owner.reopen().unwrap();
        try_exclusive(owner.as_file()).unwrap();
        assert_eq!(try_exclusive(&other).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        fs2::FileExt::unlock(owner.as_file()).unwrap();
        try_exclusive(&other).unwrap();
    }
}
