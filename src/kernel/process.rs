use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use x86_64::structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Registers {
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub r11: usize,
    pub r10: usize,
    pub r9: usize,
    pub r8: usize,
    pub rbp: usize,
    pub rdi: usize,
    pub rsi: usize,
    pub rdx: usize,
    pub rcx: usize,
    pub rbx: usize,
    pub rax: usize,
    pub rip: usize,
    pub cs: usize,
    pub rflags: usize,
    pub rsp: usize,
    pub ss: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub rbx: usize,
    pub rbp: usize,
    pub rsp: usize,
    pub rip: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Blocked,
    Zombie,
}

pub struct Process {
    pub pid: usize,
    pub ppid: usize,
    pub state: ProcessState,
    pub context: Context,
    pub trap_frame: Option<Registers>,
    pub cr3: PhysFrame<Size4KiB>,
    pub page_table: Arc<Mutex<OffsetPageTable<'static>>>,
    pub file_descriptors: Vec<Option<Arc<dyn FileOps>>>,
    pub cwd_ino: usize,
    pub exit_code: i32,
}

pub trait FileOps: Send + Sync {
    fn read(&self, buf: &mut [u8]) -> Result<usize, usize>;
    fn write(&self, buf: &[u8]) -> Result<usize, usize>;
}

pub static PROCESS_TABLE: spin::Lazy<Mutex<Vec<Option<Arc<Mutex<Process>>>>>> = 
    spin::Lazy::new(|| Mutex::new(Vec::new()));

pub static CURRENT_PID: AtomicUsize = AtomicUsize::new(0);

pub static FUTEX_QUEUE: spin::Lazy<Mutex<BTreeMap<u64, VecDeque<usize>>>> =
    spin::Lazy::new(|| Mutex::new(BTreeMap::new()));

pub fn register_process(process: Process) -> usize {
    let mut table = PROCESS_TABLE.lock();
    let pid = table.len();
    table.push(Some(Arc::new(Mutex::new(process))));
    pid
}

pub fn get_process(pid: usize) -> Option<Arc<Mutex<Process>>> {
    let table = PROCESS_TABLE.lock();
    table.get(pid).cloned().flatten()
}

pub fn current_pid() -> usize {
    CURRENT_PID.load(Ordering::Relaxed)
}

pub fn switch_to(prev: &mut Context, next: &Context) {
    unsafe {
        crate::arch::x86_64::switch_context(prev as *mut Context, next as *const Context);
    }
}

pub fn sys_futex(uaddr: u64, op: u32, val: u32) -> i32 {
    let cur_pid = current_pid();
    if op == 0 {
        let val_at_addr = unsafe { *(uaddr as *const u32) };
        if val_at_addr != val {
            return -11;
        }
        let mut q = FUTEX_QUEUE.lock();
        q.entry(uaddr).or_default().push_back(cur_pid);
        drop(q);

        let table = PROCESS_TABLE.lock();
        if let Some(Some(p)) = table.get(cur_pid) {
            p.lock().state = ProcessState::Blocked;
        }
        drop(table);
        schedule();
        0
    } else if op == 1 {
        let mut q = FUTEX_QUEUE.lock();
        let mut woken = 0;
        if let Some(waiters) = q.get_mut(&uaddr) {
            for _ in 0..val {
                if let Some(pid) = waiters.pop_front() {
                    let table = PROCESS_TABLE.lock();
                    if let Some(Some(p)) = table.get(pid) {
                        p.lock().state = ProcessState::Ready;
                        woken += 1;
                    }
                } else {
                    break;
                }
            }
        }
        woken
    } else {
        -22
    }
}

pub fn sys_mmap(addr: u64, len: usize, _prot: u32, _flags: u32, _fd: i32, _offset: usize) -> Result<u64, &'static str> {
    let pages_count = (len as u64 + 4095) / 4096;
    let start_addr = if addr == 0 {
        0x0000_0008_0000_0000
    } else {
        addr
    };

    let mut map_guard = crate::kernel::MAPPER.lock();
    let mut alloc_guard = crate::kernel::FRAME_ALLOCATOR.lock();
    let (mapper, allocator) = match (map_guard.as_mut(), alloc_guard.as_mut()) {
        (Some(m), Some(a)) => (m, a),
        _ => return Err("MMU unavailable"),
    };

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    for i in 0..pages_count {
        let page_addr = VirtAddr::new(start_addr + i * 4096);
        let page: Page<Size4KiB> = Page::containing_address(page_addr);
        let _ = mapper.unmap(page);

        let frame = allocator.allocate_frame().ok_or("OOM")?;
        unsafe {
            if let Ok(flush) = mapper.map_to(page, frame, flags, allocator) {
                flush.flush();
            }
        }
    }
    Ok(start_addr)
}

pub fn sys_clone(rip: u64, rsp: u64) -> Result<usize, &'static str> {
    let cur_pid = current_pid();
    let mut table = PROCESS_TABLE.lock();

    let (cr3, pt, cwd) = if let Some(Some(p)) = table.get(cur_pid) {
        let p_lock = p.lock();
        (p_lock.cr3, p_lock.page_table.clone(), p_lock.cwd_ino)
    } else {
        return Err("No active process to clone");
    };

    let new_pid = match table.iter().position(|slot| slot.is_none()) {
        Some(idx) => idx,
        None => {
            let idx = table.len();
            table.push(None);
            idx
        }
    };

    let mut trap_frame = Registers::default();
    trap_frame.rip = rip as usize;
    trap_frame.rsp = rsp as usize;
    trap_frame.cs = 0x23;
    trap_frame.ss = 0x1B;
    trap_frame.rflags = 0x202;

    let process = Process {
        pid: new_pid,
        ppid: cur_pid,
        state: ProcessState::Ready,
        context: Context::default(),
        trap_frame: Some(trap_frame),
        cr3,
        page_table: pt,
        file_descriptors: Vec::new(),
        cwd_ino: cwd,
        exit_code: 0,
    };

    table[new_pid] = Some(Arc::new(Mutex::new(process)));
    Ok(new_pid)
}

pub fn sys_exit_thread() {
    let cur_pid = current_pid();
    let table = PROCESS_TABLE.lock();
    if let Some(Some(p)) = table.get(cur_pid) {
        p.lock().state = ProcessState::Zombie;
    }
    drop(table);
    schedule();
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn schedule() {
    let table = PROCESS_TABLE.lock();
    if table.is_empty() {
        return;
    }

    let cur_pid = CURRENT_PID.load(Ordering::Relaxed);
    let total = table.len();
    let mut next_pid = None;

    for i in 1..=total {
        let idx = (cur_pid + i) % total;
        if let Some(Some(proc_arc)) = table.get(idx) {
            let proc = proc_arc.lock();
            if proc.state == ProcessState::Ready {
                next_pid = Some(idx);
                break;
            }
        }
    }

    let next_idx = match next_pid {
        Some(idx) => idx,
        None => return,
    };

    if next_idx == cur_pid {
        return;
    }

    let (mut prev_ctx, next_ctx, next_cr3) = {
        let cur_proc_arc = match table.get(cur_pid).and_then(|p| p.as_ref()) {
            Some(p) => p.clone(),
            None => return,
        };
        let next_proc_arc = match table.get(next_idx).and_then(|p| p.as_ref()) {
            Some(p) => p.clone(),
            None => return,
        };

        let mut cur_guard = cur_proc_arc.lock();
        let mut next_guard = next_proc_arc.lock();

        if cur_guard.state == ProcessState::Running {
            cur_guard.state = ProcessState::Ready;
        }
        next_guard.state = ProcessState::Running;

        (cur_guard.context, next_guard.context, next_guard.cr3)
    };

    CURRENT_PID.store(next_idx, Ordering::Relaxed);
    drop(table);

    unsafe {
        use x86_64::registers::control::Cr3;
        let (current_cr3, flags) = Cr3::read();
        if current_cr3 != next_cr3 {
            Cr3::write(next_cr3, flags);
        }
    }

    switch_to(&mut prev_ctx, &next_ctx);
}