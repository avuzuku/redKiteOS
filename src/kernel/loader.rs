use alloc::vec::Vec;
use core::mem::size_of;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, Translate,
};
use x86_64::VirtAddr;
use crate::kernel::error::ProcessStatus;
use crate::sys::fs::VFS;

pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const DEFAULT_USER_STACK_PAGES: u64 = 8;
pub const PAGE_SIZE: u64 = 4096;

pub const DEFAULT_PATH: &[&str] = &["/bin", "/usr/bin", "."];

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub fn deploy_binaries() -> Result<(), &'static str> {
    let fs = VFS.lock();
    fs.create_dir(0, "bin", 0, 0).ok();
    Ok(())
}

pub fn execute_file(cwd_ino: usize, path_or_name: &str) -> Result<ProcessStatus, &'static str> {
    let (ino, perms) = resolve_binary(cwd_ino, path_or_name)?;

    if (perms & 0o111) == 0 {
        return Err("Permission denied (EACCES)");
    }

    let raw_bytes = {
        let fs = VFS.lock();
        fs.read_inode_data(ino).map_err(|_| "Storage read fault (EIO)")?
    };

    if raw_bytes.len() < size_of::<Elf64Header>() {
        return Err("Truncated binary image (ENOEXEC)");
    }

    let header = unsafe { core::ptr::read_unaligned(raw_bytes.as_ptr() as *const Elf64Header) };

    if header.e_ident[0..4] != [0x7F, b'E', b'L', b'F'] {
        return Err("Not a valid ELF64 executable (ENOEXEC)");
    }

    let mut map_guard = crate::kernel::MAPPER.lock();
    let mut alloc_guard = crate::kernel::FRAME_ALLOCATOR.lock();
    let (mapper, allocator) = match (map_guard.as_mut(), alloc_guard.as_mut()) {
        (Some(m), Some(a)) => (m, a),
        _ => return Err("MMU subsystem unavailable"),
    };

    let ph_offset = header.e_phoff as usize;
    let ph_num = header.e_phnum as usize;
    let ph_ent_size = header.e_phentsize as usize;

    let mut allocated_pages: Vec<Page<Size4KiB>> = Vec::new();

    for i in 0..ph_num {
        let phdr_offset = ph_offset + i * ph_ent_size;
        if phdr_offset + size_of::<Elf64Phdr>() > raw_bytes.len() {
            continue;
        }
        let phdr = unsafe { core::ptr::read_unaligned(raw_bytes.as_ptr().add(phdr_offset) as *const Elf64Phdr) };

        if phdr.p_type == 1 {
            let start_page = phdr.p_vaddr / PAGE_SIZE;
            let end_page = (phdr.p_vaddr + phdr.p_memsz + PAGE_SIZE - 1) / PAGE_SIZE;
            
            let mut final_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
            if (phdr.p_flags & 2) != 0 {
                final_flags |= PageTableFlags::WRITABLE;
            }

            let init_flags = final_flags | PageTableFlags::WRITABLE;

            for page_idx in start_page..end_page {
                let page_addr = VirtAddr::new(page_idx * PAGE_SIZE);
                let page: Page<Size4KiB> = Page::containing_address(page_addr);

                if !allocated_pages.contains(&page) {
                    if mapper.translate_page(page).is_ok() {
                        for p in &allocated_pages {
                            if let Ok((_, flush)) = mapper.unmap(*p) {
                                flush.flush();
                            }
                        }
                        return Err("ELF overlap with kernel memory (relink with --image-base)!");
                    }
                    
                    let frame = allocator.allocate_frame().ok_or("OOM mapping ELF")?;
                    unsafe {
                        if let Ok(flush) = mapper.map_to(page, frame, init_flags, allocator) {
                            flush.flush();
                        }
                        core::ptr::write_bytes(page.start_address().as_mut_ptr::<u8>(), 0, PAGE_SIZE as usize);
                    }
                    allocated_pages.push(page);
                } else {
                    unsafe {
                        if let Ok(flush) = mapper.update_flags(page, init_flags) {
                            flush.flush();
                        }
                    }
                }
            }

            unsafe {
                let dest = phdr.p_vaddr as *mut u8;
                if phdr.p_filesz > 0 && (phdr.p_offset as usize + phdr.p_filesz as usize) <= raw_bytes.len() {
                    core::ptr::copy_nonoverlapping(
                        raw_bytes.as_ptr().add(phdr.p_offset as usize),
                        dest,
                        phdr.p_filesz as usize,
                    );
                }
                if phdr.p_memsz > phdr.p_filesz {
                    core::ptr::write_bytes(
                        dest.add(phdr.p_filesz as usize),
                        0,
                        (phdr.p_memsz - phdr.p_filesz) as usize,
                    );
                }
            }

            for page_idx in start_page..end_page {
                let page_addr = VirtAddr::new(page_idx * PAGE_SIZE);
                let page: Page<Size4KiB> = Page::containing_address(page_addr);
                unsafe {
                    if let Ok(flush) = mapper.update_flags(page, final_flags) {
                        flush.flush();
                    }
                }
            }
        }
    }

    let stack_base = USER_STACK_TOP - (DEFAULT_USER_STACK_PAGES * PAGE_SIZE);
    let stack_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;

    for i in 0..DEFAULT_USER_STACK_PAGES {
        let page_addr = VirtAddr::new(stack_base + i * PAGE_SIZE);
        let page: Page<Size4KiB> = Page::containing_address(page_addr);

        if mapper.translate_page(page).is_ok() {
            for p in &allocated_pages {
                if let Ok((_, flush)) = mapper.unmap(*p) {
                    flush.flush();
                }
            }
            return Err("Stack overlap with kernel memory!");
        }

        let frame = allocator.allocate_frame().ok_or("OOM for stack")?;
        unsafe {
            if let Ok(flush) = mapper.map_to(page, frame, stack_flags, allocator) {
                flush.flush();
            }
            core::ptr::write_bytes(page.start_address().as_mut_ptr::<u8>(), 0, PAGE_SIZE as usize);
        }
        allocated_pages.push(page);
    }

    let entry_point = header.e_entry;

    drop(map_guard);
    drop(alloc_guard);

    crate::kernel::syscall::enter_userspace(entry_point, USER_STACK_TOP)
}

fn resolve_binary(cwd_ino: usize, name: &str) -> Result<(usize, u16), &'static str> {
    let fs = VFS.lock();

    if name.starts_with('/') {
        if let Ok(ino) = fs.resolve_path(0, name) {
            if let Ok(inod) = fs.read_inode_public(ino) {
                return Ok((ino, inod.permissions));
            }
        }
        return Err("Executable not found by absolute path");
    }

    for &dir in DEFAULT_PATH {
        let base_ino = if dir == "." {
            cwd_ino
        } else {
            match fs.resolve_path(0, dir) {
                Ok(ino) => ino,
                Err(_) => continue,
            }
        };

        if let Ok(target_ino) = fs.resolve_path(base_ino, name) {
            if let Ok(inod) = fs.read_inode_public(target_ino) {
                return Ok((target_ino, inod.permissions));
            }
        }
    }

    Err("Executable not found in PATH")
}