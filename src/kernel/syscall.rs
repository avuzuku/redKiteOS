use core::sync::atomic::{AtomicU64, Ordering};
use crate::kernel::error::ProcessStatus;

pub static SAVED_KERNEL_RSP: AtomicU64 = AtomicU64::new(0);
pub static EXIT_STATUS: AtomicU64 = AtomicU64::new(0);

unsafe fn wrmsr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    core::arch::asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nomem, nostack));
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack));
    ((high as u64) << 32) | (low as u64)
}

pub fn init() {
    unsafe {
        let efer = rdmsr(0xC000_0080);
        wrmsr(0xC000_0080, efer | 1);
        let star = ((0x0010u64 | 3) << 48) | (0x0008u64 << 32);
        wrmsr(0xC000_0081, star);
        wrmsr(0xC000_0082, syscall_entry as *const () as usize as u64);
        wrmsr(0xC000_0084, 0x200);
    }
}

#[no_mangle]
pub extern "C" fn syscall_dispatch(
    id: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
    _arg5: usize,
) -> usize {
    match id {
        0 => {
            let buf = arg1 as *mut u8;
            let max = arg2;
            if let Some(key) = crate::kernel::task::pop_key() {
                match key {
                    pc_keyboard::DecodedKey::Unicode(c) => {
                        if !buf.is_null() && max > 0 {
                            unsafe { *buf = c as u8; }
                            1
                        } else {
                            c as usize
                        }
                    }
                    _ => 0,
                }
            } else {
                0
            }
        }
        1 => {
            let (ptr, len) = if arg1 <= 2 && arg2 >= 0x1000 {
                (arg2 as *const u8, arg3)
            } else {
                (arg1 as *const u8, arg2.min(4096))
            };
            if ptr.is_null() {
                (-14isize) as usize
            } else if len == 0 {
                0
            } else {
                let s = unsafe { core::slice::from_raw_parts(ptr, len) };
                if let Ok(st) = core::str::from_utf8(s) {
                    crate::print!("{}", st);
                    len
                } else {
                    (-22isize) as usize
                }
            }
        }
        2 => {
            if let Some(key) = crate::kernel::task::pop_key() {
                match key {
                    pc_keyboard::DecodedKey::Unicode(c) => c as usize,
                    _ => 0,
                }
            } else {
                0
            }
        }
        3 | 35 => {
            crate::kernel::timer::sleep_ticks(arg1 as u64);
            0
        }
        4 | 60 => {
            EXIT_STATUS.store(arg1 as u64, Ordering::SeqCst);
            let k_rsp = SAVED_KERNEL_RSP.load(Ordering::SeqCst);
            unsafe {
                core::arch::asm!(
                    "mov rsp, {0}",
                    "ret",
                    in(reg) k_rsp,
                    options(noreturn)
                );
            }
        }
        5 => crate::kernel::timer::ticks() as usize,
        9 => {
            match crate::kernel::process::sys_mmap(arg1 as u64, arg2, arg3 as u32, arg4 as u32, 0, 0) {
                Ok(addr) => addr as usize,
                Err(_) => (-12isize) as usize,
            }
        }
        10 => {
            let width = arg1;
            let height = arg2;
            let title_ptr = arg3 as *const u8;
            let title_len = arg4.min(64);
            let title = if !title_ptr.is_null() && title_len > 0 {
                let s = unsafe { core::slice::from_raw_parts(title_ptr, title_len) };
                core::str::from_utf8(s).unwrap_or("App")
            } else {
                "App"
            };
            match crate::kernel::ipc::sys_create_window(width, height, title) {
                Ok(win_id) => win_id,
                Err(_) => (-22isize) as usize,
            }
        }
        11 => {
            let win_id = arg1;
            match crate::kernel::ipc::sys_map_window_buffer(win_id) {
                Ok(addr) => addr as usize,
                Err(_) => (-9isize) as usize,
            }
        }
        12 => {
            let win_id = arg1;
            let ev_ptr = arg2 as *mut crate::kernel::ipc::WindowEvent;
            match crate::kernel::ipc::sys_poll_event(win_id, ev_ptr) {
                Ok(count) => count,
                Err(_) => (-9isize) as usize,
            }
        }
        13 => {
            let win_id = arg1;
            match crate::kernel::ipc::destroy_window(win_id) {
                Ok(_) => 0,
                Err(_) => (-9isize) as usize,
            }
        }
        20 | 39 => crate::kernel::process::current_pid(),
        24 => {
            crate::kernel::process::schedule();
            0
        }
        40 => {
            match crate::drivers::net::sys_socket(arg1 as i32, arg2 as i32, arg3 as i32) {
                Ok(fd) => fd as usize,
                Err(e) => (e as isize) as usize,
            }
        }
        41 => {
            match crate::drivers::net::sys_connect(arg1 as i32, arg2 as *const crate::drivers::net::sockaddr, arg3 as u32) {
                Ok(()) => 0,
                Err(e) => (e as isize) as usize,
            }
        }
        42 => {
            match crate::drivers::net::sys_send(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i32) {
                Ok(len) => len,
                Err(e) => (e as isize) as usize,
            }
        }
        43 => {
            match crate::drivers::net::sys_recv(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as i32) {
                Ok(len) => len,
                Err(e) => (e as isize) as usize,
            }
        }
        56 => {
            match crate::kernel::process::sys_clone(arg1 as u64, arg2 as u64) {
                Ok(pid) => pid,
                Err(_) => (-12isize) as usize,
            }
        }
        57 => {
            crate::kernel::process::sys_exit_thread();
            0
        }
        202 => {
            (crate::kernel::process::sys_futex(arg1 as u64, arg2 as u32, arg3 as u32) as isize) as usize
        }
        _ => (-38isize) as usize,
    }
}

#[no_mangle]
pub extern "C" fn syscall_dispatcher(id: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    syscall_dispatch(id as usize, arg1 as usize, arg2 as usize, arg3 as usize, 0, 0) as u64
}

#[unsafe(naked)]
unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        "push rcx", "push r11", "push rbp", "push rbx", "push r12", "push r13", "push r14", "push r15",
        "mov r9, r8",
        "mov r8, r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",
        "call syscall_dispatch",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbx", "pop rbp", "pop r11", "pop rcx",
        "sysretq"
    );
}

#[unsafe(naked)]
unsafe extern "C" fn userspace_trampoline(
    saved_rsp_ptr: *const AtomicU64,
    code_addr: u64,
    stack_addr: u64,
) {
    core::arch::naked_asm!(
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "lea rax, [rip + 1f]",
        "push rax",
        "mov [rdi], rsp",
        "push 0x1B",
        "push rdx",
        "push 0x202",
        "push 0x23",
        "push rsi",
        "iretq",
        "1:",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret"
    );
}

pub fn enter_userspace(code_addr: u64, stack_addr: u64) -> Result<ProcessStatus, &'static str> {
    unsafe {
        userspace_trampoline(
            &SAVED_KERNEL_RSP as *const AtomicU64,
            code_addr,
            stack_addr,
        );
    }
    x86_64::instructions::interrupts::enable();
    let code = EXIT_STATUS.load(Ordering::SeqCst);
    let status = match code {
        0 => ProcessStatus::Success,
        13 => ProcessStatus::PermissionDenied,
        22 => ProcessStatus::InvalidArguments,
        126 => ProcessStatus::ExecutionFailed,
        127 => ProcessStatus::NotFound,
        139 => ProcessStatus::Crashed,
        _ => ProcessStatus::ExecutionFailed,
    };
    Ok(status)
}