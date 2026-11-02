use crate::println;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Errno {
    EPERM = 1,
    ENOENT = 2,
    ESRCH = 3,
    EINTR = 4,
    EIO = 5,
    ENXIO = 6,
    ENOEXEC = 8,
    EBADF = 9,
    ECHILD = 10,
    EAGAIN = 11,
    ENOMEM = 12,
    EACCES = 13,
    EFAULT = 14,
    EBUSY = 16,
    EEXIST = 17,
    EXDEV = 18,
    ENODEV = 19,
    ENOTDIR = 20,
    EISDIR = 21,
    EINVAL = 22,
    ENFILE = 23,
    EMFILE = 24,
    ENOSPC = 28,
    ESPIPE = 29,
    EROFS = 30,
    EMLINK = 31,
    EPIPE = 32,
    ENOSYS = 38,
    ELOOP = 40,
    ETIMEDOUT = 110,
    ECRASH = 139,
}

impl Errno {
    pub fn as_str(self) -> &'static str {
        match self {
            Errno::EPERM => "B-Baka! Operation not permitted!",
            Errno::ENOENT => "Where are you looking?! No such file or directory, baka!",
            Errno::ESRCH => "No such process, Senpai! Did it vanish into another dimension?",
            Errno::EINTR => "Yamete! System call interrupted!",
            Errno::EIO => "Hardware screaming! I/O error on ATA bus!",
            Errno::ENXIO => "No such device or address! You hallucinating?",
            Errno::ENOEXEC => "B-Baka! That's not a valid binary executable! What did you give me?!",
            Errno::EBADF => "Bad file descriptor! Don't feed me garbage handles!",
            Errno::ECHILD => "No child processes left to adopt!",
            Errno::EAGAIN => "Resource busy! Try again later, impatient baka!",
            Errno::ENOMEM => "Wheres my RAM, Senpai?! Kernel allocator starved!",
            Errno::EACCES => "Permission denied! You are not allowed in here!",
            Errno::EFAULT => "Shinigami memory trespass! Bad virtual pointer address!",
            Errno::EBUSY => "Device is busy doing important things!",
            Errno::EEXIST => "File already exists! Stop overwriting things blindly!",
            Errno::EXDEV => "Cannot cross-link across different device boundaries!",
            Errno::ENODEV => "Silicon missing! Device not found on motherboard!",
            Errno::ENOTDIR => "Baka! That's a file, not a directory!",
            Errno::EISDIR => "That's a whole folder! You can't read it like a plain file!",
            Errno::EINVAL => "Invalid argument! The math makes no sense!",
            Errno::ENFILE => "System file table is overflowing! Calm down!",
            Errno::EMFILE => "Too many open descriptors for one process!",
            Errno::ENOSPC => "No space left on KiteFS! Did you download the whole internet?!",
            Errno::ESPIPE => "Illegal seek operation!",
            Errno::EROFS => "Read-only filesystem! Hands off!",
            Errno::EMLINK => "Too many hard-links attached to inode!",
            Errno::EPIPE => "Broken pipe! The other end gave up and left!",
            Errno::ENOSYS => "Senpai, that syscall doesn't exist yet! Don't make things up! (¬_¬)",
            Errno::ELOOP => "Too many levels of symbolic links, baka! Did you trap us in an infinite timeloop?!",
            Errno::ETIMEDOUT => "Waited for you forever, Senpai! Connection timed out! (T_T)",
            Errno::ECRASH => "Process suffered an existential crisis and died! (x_x)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ProcessStatus {
    Success = 0,
    InvalidArguments = 22,
    PermissionDenied = 13,
    ExecutionFailed = 126,
    NotFound = 127,
    Crashed = 139,
}

pub fn handle_process_termination(app_name: &str, status: ProcessStatus, detail: &str) {
    if status != ProcessStatus::Success {
        println!();
        println!("==================================================");
        println!("  [!] PROCESS SUBSYSTEM: APPLICATION ERROR REPORT ");
        println!("==================================================");
        println!("B-Baka! Process '{}' failed with code {:?}!", app_name, status);
        println!("Senpai's diagnostic: {}", detail);
        println!("Kernel state: ISOLATED & PROTECTED. Ring 0 is safe.");
        println!("Returning to shell before you break anything else!");
        println!("==================================================");
    }
}

pub fn report_app_failure(app_name: &str, reason: &str) {
    println!();
    println!("==================================================");
    println!("  [!] PROCESS SUBSYSTEM: YAMETE KUDASAI! (x_x)    ");
    println!("==================================================");
    println!("B-Baka! What did you do to '{}'?!", app_name);
    println!("Crash dump: {}", reason);
    println!("Kernel state: Protected by Ring 0 barrier. No panic.");
    println!("Cleaned up the mess. Returning to shell prompt.");
    println!("==================================================");
}