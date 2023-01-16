use std::{fs::File, io::Error};

/// Reset stdin and stdout to the attached console / tty for the duration of the closure.
/// If no console is available, stdin and stdout will be redirected to null.
pub fn stdin_stdout_to_console<F, T>(f: F) -> Result<T, Error>
where
    F: FnOnce() -> T,
{
    let open_write = |f| std::fs::OpenOptions::new().write(true).open(f);

    let mut stdin = File::open(imp::IN_DEVICE).or_else(|_| File::open(imp::NULL_DEVICE))?;
    let mut stdout = open_write(imp::OUT_DEVICE).or_else(|_| open_write(imp::NULL_DEVICE))?;

    let _stdin_guard = imp::ReplacementGuard::new(Stdio::Stdin, &mut stdin)?;
    let _stdout_guard = imp::ReplacementGuard::new(Stdio::Stdout, &mut stdout)?;
    Ok(f())
}

enum Stdio {
    Stdin,
    Stdout,
}

#[cfg(windows)]
mod imp {
    use super::Stdio;
    use std::{fs::File, io::Error, os::windows::prelude::AsRawHandle};
    use windows_sys::Win32::{
        Foundation::{HANDLE, INVALID_HANDLE_VALUE},
        System::Console::{
            GetStdHandle, SetStdHandle, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        },
    };
    pub const OUT_DEVICE: &str = "CONOUT$";
    pub const IN_DEVICE: &str = "CONIN$";
    pub const NULL_DEVICE: &str = "NUL";

    /// Restores previous stdio when dropped.
    pub struct ReplacementGuard {
        std_handle: STD_HANDLE,
        previous: HANDLE,
    }

    impl ReplacementGuard {
        pub(super) fn new(stdio: Stdio, replacement: &mut File) -> Result<ReplacementGuard, Error> {
            let std_handle = match stdio {
                Stdio::Stdin => STD_INPUT_HANDLE,
                Stdio::Stdout => STD_OUTPUT_HANDLE,
            };

            let previous;
            unsafe {
                // Make a copy of the current handle
                previous = GetStdHandle(std_handle);
                if previous == INVALID_HANDLE_VALUE {
                    return Err(std::io::Error::last_os_error());
                }

                // Replace stdin with the replacement handle
                if SetStdHandle(std_handle, replacement.as_raw_handle() as HANDLE) == 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }

            Ok(ReplacementGuard {
                previous,
                std_handle,
            })
        }
    }

    impl Drop for ReplacementGuard {
        fn drop(&mut self) {
            unsafe {
                // Put previous handle back in to stdin
                SetStdHandle(self.std_handle, self.previous);
            }
        }
    }
}

#[cfg(unix)]
mod imp {
    use super::Stdio;
    use rustix::fd::OwnedFd;
    use rustix::io::dup;
    use rustix::stdio::{dup2_stdin, dup2_stdout, stdin, stdout};
    use std::{fs::File, io::Error};
    pub const IN_DEVICE: &str = "/dev/tty";
    pub const OUT_DEVICE: &str = "/dev/tty";
    pub const NULL_DEVICE: &str = "/dev/null";

    /// Restores previous stdio when dropped.
    pub struct ReplacementGuard {
        stdio: Stdio,
        previous: OwnedFd,
    }

    impl ReplacementGuard {
        pub(super) fn new(stdio: Stdio, replacement: &mut File) -> Result<ReplacementGuard, Error> {
            // Duplicate the existing stdin file to a new descriptor
            let previous = match stdio {
                Stdio::Stdin => dup(stdin())?,
                Stdio::Stdout => dup(stdout())?,
            };

            // Replace stdin with the replacement file
            match stdio {
                Stdio::Stdin => dup2_stdin(replacement)?,
                Stdio::Stdout => dup2_stdout(replacement)?,
            }

            Ok(ReplacementGuard { previous, stdio })
        }
    }

    impl Drop for ReplacementGuard {
        fn drop(&mut self) {
            // Put previous file back in to stdin
            match self.stdio {
                Stdio::Stdin => dup2_stdin(&self.previous).unwrap(),
                Stdio::Stdout => dup2_stdout(&self.previous).unwrap(),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::fs::OpenOptions;
    use std::io::{Seek, Write};

    use super::imp::ReplacementGuard;
    use super::Stdio;

    #[test]
    fn stdin() {
        let tempdir = snapbox::path::PathFixture::mutable_temp().unwrap();
        let file = tempdir.path().unwrap().join("stdin");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(file)
            .unwrap();

        writeln!(&mut file, "hello").unwrap();
        file.seek(std::io::SeekFrom::Start(0)).unwrap();
        {
            let _guard = ReplacementGuard::new(Stdio::Stdin, &mut file).unwrap();
            let line = std::io::stdin().lines().next().unwrap().unwrap();
            assert_eq!(line, "hello");
        }
    }
}
