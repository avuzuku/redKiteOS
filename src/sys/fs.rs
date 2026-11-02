use alloc::string::String;
use alloc::vec::Vec;
use core::mem::size_of;
use spin::{Lazy, Mutex};
use crate::kernel::error::Errno;

pub const SECTOR_SIZE: usize = 512;
const FS_SUPERBLOCK_LBA: u32 = 1000;
const FS_INODE_START_LBA: u32 = 1001;
const FS_DATA_START_LBA: u32 = 1009;
const MAGIC: u32 = 0x4B495445;
const MAX_INODES: usize = 64;
const MAX_SYMLINK_DEPTH: usize = 8;

pub const DEV_NULL: u32 = 1;
pub const DEV_ATA0: u32 = 2;
pub const DEV_MOUSE: u32 = 3;
pub const DEV_KBD: u32 = 4;
pub const DEV_FB0: u32 = 5;

#[derive(Clone, PartialEq, Eq)]
pub struct DesktopShortcut {
    pub name: String,
    pub inode: u32,
    pub is_executable: bool,
    pub path: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InodeType {
    Unused = 0,
    Regular = 1,
    Directory = 2,
    CharDev = 3,
    BlockDev = 4,
    Fifo = 5,
    Symlink = 6,
    Socket = 7,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SuperBlock {
    magic: u32,
    total_inodes: u32,
    free_blocks: u32,
    next_free_block: u32,
    _pad: [u8; 496],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Inode {
    pub file_type: u8,
    pub _pad0: u8,
    pub permissions: u16,
    pub nlink: u16,
    pub indirect_block: u16,
    pub size: u32,
    pub blocks: [u16; 6],
    pub uid: u32,
    pub gid: u32,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub rdev: u32,
    pub _pad1: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DirEntry {
    pub inode: u32,
    pub name: [u8; 252],
}

pub struct Vfs;

impl Vfs {
    pub fn format(&mut self) -> Result<(), Errno> {
        let sb = SuperBlock {
            magic: MAGIC,
            total_inodes: MAX_INODES as u32,
            free_blocks: 200,
            next_free_block: 0,
            _pad: [0; 496],
        };

        let mut sb_buf = [0u8; SECTOR_SIZE];
        unsafe {
            core::ptr::copy_nonoverlapping(
                &sb as *const SuperBlock as *const u8,
                sb_buf.as_mut_ptr(),
                size_of::<SuperBlock>(),
            );
        }
        crate::drivers::ata::write_sector(FS_SUPERBLOCK_LBA, &sb_buf)?;

        let empty = [0u8; SECTOR_SIZE];
        for i in 0..8 {
            crate::drivers::ata::write_sector(FS_INODE_START_LBA + i, &empty)?;
        }

        self.create_inode(0, InodeType::Directory, 0o755, 0, 0, 0)?;
        self.create_inode(1, InodeType::Directory, 0o755, 0, 0, 0)?;
        self.create_inode(2, InodeType::Directory, 0o755, 0, 0, 0)?;
        self.create_inode(3, InodeType::Directory, 0o755, 0, 0, 0)?;
        self.create_inode(4, InodeType::Directory, 0o755, 0, 0, 0)?;

        self.add_dir_entry(0, 0, ".")?;
        self.add_dir_entry(0, 0, "..")?;
        self.add_dir_entry(0, 1, "bin")?;
        self.add_dir_entry(0, 2, "etc")?;
        self.add_dir_entry(0, 3, "home")?;
        self.add_dir_entry(0, 4, "dev")?;

        self.add_dir_entry(1, 1, ".")?;
        self.add_dir_entry(1, 0, "..")?;

        self.add_dir_entry(2, 2, ".")?;
        self.add_dir_entry(2, 0, "..")?;

        self.add_dir_entry(3, 3, ".")?;
        self.add_dir_entry(3, 0, "..")?;

        self.add_dir_entry(4, 4, ".")?;
        self.add_dir_entry(4, 0, "..")?;

        let ata_ino = self.alloc_inode(InodeType::BlockDev, 0o660, 0, 0, DEV_ATA0)?;
        self.add_dir_entry(4, ata_ino as u32, "ata0")?;

        let null_ino = self.alloc_inode(InodeType::CharDev, 0o666, 0, 0, DEV_NULL)?;
        self.add_dir_entry(4, null_ino as u32, "null")?;

        let mouse_ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_MOUSE)?;
        self.add_dir_entry(4, mouse_ino as u32, "mouse")?;

        let kbd_ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_KBD)?;
        self.add_dir_entry(4, kbd_ino as u32, "kbd")?;

        let fb_ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_FB0)?;
        self.add_dir_entry(4, fb_ino as u32, "fb0")?;

        let hostname_ino = self.alloc_inode(InodeType::Regular, 0o644, 0, 0, 0)?;
        self.add_dir_entry(2, hostname_ino as u32, "hostname")?;
        self.write_inode_data(hostname_ino, b"redkite-os\n")?;

        let desktop_ino = self.alloc_inode(InodeType::Directory, 0o755, 1000, 1000, 0)?;
        self.add_dir_entry(desktop_ino, desktop_ino as u32, ".")?;
        self.add_dir_entry(desktop_ino, 3, "..")?;
        self.add_dir_entry(3, desktop_ino as u32, "Desktop")?;

        let initrc_ino = self.alloc_inode(InodeType::Regular, 0o644, 1000, 1000, 0)?;
        self.add_dir_entry(3, initrc_ino as u32, ".initrc")?;
        self.write_inode_data(initrc_ino, b"echo Welcome to redKiteOS, Senpai!\n")?;

        Ok(())
    }

    pub fn init(&mut self) -> Result<(), Errno> {
        let mut buf = [0u8; SECTOR_SIZE];
        match crate::drivers::ata::read_sector(FS_SUPERBLOCK_LBA, &mut buf) {
            Ok(()) => {
                let sb: SuperBlock = unsafe { (buf.as_ptr() as *const SuperBlock).read_unaligned() };
                if sb.magic != MAGIC {
                    self.format()?;
                } else {
                    self.ensure_dev_nodes()?;
                }
            }
            Err(_) => {
                self.format()?;
            }
        }
        Ok(())
    }

    fn ensure_dev_nodes(&self) -> Result<(), Errno> {
        if let Ok(dev_ino) = self.resolve_path(0, "/dev") {
            if self.resolve_path(dev_ino, "mouse").is_err() {
                let ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_MOUSE)?;
                let _ = self.add_dir_entry(dev_ino, ino as u32, "mouse");
            }
            if self.resolve_path(dev_ino, "kbd").is_err() {
                let ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_KBD)?;
                let _ = self.add_dir_entry(dev_ino, ino as u32, "kbd");
            }
            if self.resolve_path(dev_ino, "fb0").is_err() {
                let ino = self.alloc_inode(InodeType::CharDev, 0o660, 0, 0, DEV_FB0)?;
                let _ = self.add_dir_entry(dev_ino, ino as u32, "fb0");
            }
        }
        if let Ok(home_ino) = self.resolve_path(0, "/home") {
            if self.resolve_path(home_ino, "Desktop").is_err() {
                if let Ok(desktop_ino) = self.alloc_inode(InodeType::Directory, 0o755, 1000, 1000, 0) {
                    let _ = self.add_dir_entry(desktop_ino, desktop_ino as u32, ".");
                    let _ = self.add_dir_entry(desktop_ino, home_ino as u32, "..");
                    let _ = self.add_dir_entry(home_ino, desktop_ino as u32, "Desktop");
                }
            }
            if self.resolve_path(home_ino, ".initrc").is_err() {
                if let Ok(initrc_ino) = self.alloc_inode(InodeType::Regular, 0o644, 1000, 1000, 0) {
                    let _ = self.add_dir_entry(home_ino, initrc_ino as u32, ".initrc");
                    let _ = self.write_inode_data(initrc_ino, b"echo Welcome to redKiteOS, Senpai!\n");
                }
            }
        }
        Ok(())
    }

    pub fn read_inode_public(&self, idx: usize) -> Result<Inode, Errno> {
        self.read_inode(idx)
    }

    pub fn chmod(&self, idx: usize, mode: u16) -> Result<(), Errno> {
        let mut inod = self.read_inode(idx)?;
        inod.permissions = mode & 0o7777;
        self.write_inode(idx, &inod)
    }

    pub fn chown(&self, idx: usize, uid: u32, gid: u32) -> Result<(), Errno> {
        let mut inod = self.read_inode(idx)?;
        inod.uid = uid;
        inod.gid = gid;
        self.write_inode(idx, &inod)
    }

    pub fn get_device_id(&self, inode_idx: usize) -> Result<u32, Errno> {
        let inod = self.read_inode(inode_idx)?;
        if inod.file_type == InodeType::CharDev as u8 || inod.file_type == InodeType::BlockDev as u8 {
            Ok(inod.rdev)
        } else {
            Err(Errno::EINVAL)
        }
    }

    fn read_inode(&self, idx: usize) -> Result<Inode, Errno> {
        let sector = (idx * size_of::<Inode>()) / SECTOR_SIZE;
        let offset = (idx * size_of::<Inode>()) % SECTOR_SIZE;
        let mut buf = [0u8; SECTOR_SIZE];
        crate::drivers::ata::read_sector(FS_INODE_START_LBA + sector as u32, &mut buf)?;
        Ok(unsafe { (buf.as_ptr().add(offset) as *const Inode).read_unaligned() })
    }

    fn write_inode(&self, idx: usize, inode: &Inode) -> Result<(), Errno> {
        let sector = (idx * size_of::<Inode>()) / SECTOR_SIZE;
        let offset = (idx * size_of::<Inode>()) % SECTOR_SIZE;
        let mut buf = [0u8; SECTOR_SIZE];
        crate::drivers::ata::read_sector(FS_INODE_START_LBA + sector as u32, &mut buf)?;
        unsafe {
            core::ptr::copy_nonoverlapping(
                inode as *const Inode as *const u8,
                buf.as_mut_ptr().add(offset),
                size_of::<Inode>(),
            );
        }
        crate::drivers::ata::write_sector(FS_INODE_START_LBA + sector as u32, &buf)
    }

    fn alloc_block(&self) -> Result<u16, Errno> {
        let mut buf = [0u8; SECTOR_SIZE];
        crate::drivers::ata::read_sector(FS_SUPERBLOCK_LBA, &mut buf)?;
        let mut sb: SuperBlock = unsafe { (buf.as_ptr() as *const SuperBlock).read_unaligned() };

        if sb.free_blocks == 0 {
            return Err(Errno::ENOSPC);
        }

        let blk = sb.next_free_block as u16;
        sb.next_free_block += 1;
        sb.free_blocks -= 1;

        unsafe {
            core::ptr::copy_nonoverlapping(
                &sb as *const SuperBlock as *const u8,
                buf.as_mut_ptr(),
                size_of::<SuperBlock>(),
            );
        }
        crate::drivers::ata::write_sector(FS_SUPERBLOCK_LBA, &buf)?;

        let empty = [0u8; SECTOR_SIZE];
        crate::drivers::ata::write_sector(FS_DATA_START_LBA + blk as u32, &empty)?;
        Ok(blk)
    }

    fn free_block(&self, _blk: u16) -> Result<(), Errno> {
        let mut buf = [0u8; SECTOR_SIZE];
        crate::drivers::ata::read_sector(FS_SUPERBLOCK_LBA, &mut buf)?;
        let mut sb: SuperBlock = unsafe { (buf.as_ptr() as *const SuperBlock).read_unaligned() };
        sb.free_blocks += 1;
        unsafe {
            core::ptr::copy_nonoverlapping(
                &sb as *const SuperBlock as *const u8,
                buf.as_mut_ptr(),
                size_of::<SuperBlock>(),
            );
        }
        crate::drivers::ata::write_sector(FS_SUPERBLOCK_LBA, &buf)
    }

    fn create_inode(&self, idx: usize, ftype: InodeType, perms: u16, uid: u32, gid: u32, rdev: u32) -> Result<(), Errno> {
        let blk = self.alloc_block()?;
        let inode = Inode {
            file_type: ftype as u8,
            _pad0: 0,
            permissions: perms & 0o7777,
            nlink: 1,
            indirect_block: 0,
            size: 0,
            blocks: [blk, 0, 0, 0, 0, 0],
            uid,
            gid,
            atime: 0,
            mtime: 0,
            ctime: 0,
            rdev,
            _pad1: [0; 16],
        };
        self.write_inode(idx, &inode)
    }

    fn alloc_inode(&self, ftype: InodeType, perms: u16, uid: u32, gid: u32, rdev: u32) -> Result<usize, Errno> {
        for i in 1..MAX_INODES {
            let inod = self.read_inode(i)?;
            if inod.file_type == InodeType::Unused as u8 {
                self.create_inode(i, ftype, perms, uid, gid, rdev)?;
                return Ok(i);
            }
        }
        Err(Errno::ENOSPC)
    }

    fn free_inode(&self, idx: usize) -> Result<(), Errno> {
        let mut inod = self.read_inode(idx)?;
        for blk in inod.blocks.iter() {
            if *blk != 0 {
                let _ = self.free_block(*blk);
            }
        }
        if inod.indirect_block != 0 {
            let _ = self.free_block(inod.indirect_block);
        }
        inod.file_type = InodeType::Unused as u8;
        inod.size = 0;
        inod.nlink = 0;
        inod.blocks = [0; 6];
        inod.indirect_block = 0;
        self.write_inode(idx, &inod)
    }

    fn split_path<'a>(&self, cwd_inode: usize, path: &'a str) -> Result<(usize, &'a str), Errno> {
        let clean = path.trim();
        if clean.is_empty() {
            return Err(Errno::EINVAL);
        }

        if let Some(pos) = clean.rfind('/') {
            let parent_part = &clean[..pos];
            let base_name = &clean[pos + 1..];

            if base_name.is_empty() {
                return Err(Errno::EINVAL);
            }

            let parent_inode = if parent_part.is_empty() {
                0
            } else {
                self.resolve_path(cwd_inode, parent_part)?
            };

            Ok((parent_inode, base_name))
        } else {
            Ok((cwd_inode, clean))
        }
    }

    fn normalize_desktop_filename(&self, parent_inode: usize, base_name: &str, is_executable: bool) -> String {
        if let Ok(desktop_ino) = self.resolve_path(0, "/home/Desktop") {
            if parent_inode == desktop_ino {
                if !base_name.ends_with(".ks") && !base_name.ends_with(".rke") {
                    let mut s = String::from(base_name);
                    if is_executable {
                        s.push_str(".rke");
                    } else {
                        s.push_str(".ks");
                    }
                    return s;
                }
            }
        }
        String::from(base_name)
    }

    pub fn add_dir_entry(&self, dir_inode_idx: usize, target: u32, name: &str) -> Result<(), Errno> {
        let dir = self.read_inode(dir_inode_idx)?;
        if dir.file_type != InodeType::Directory as u8 {
            return Err(Errno::ENOTDIR);
        }

        let mut data = self.read_inode_data(dir_inode_idx)?;

        let mut name_buf = [0u8; 252];
        let bytes = name.as_bytes();
        let len = bytes.len().min(251);
        name_buf[..len].copy_from_slice(&bytes[..len]);
        name_buf[len] = 0;

        let entry = DirEntry {
            inode: target,
            name: name_buf,
        };

        let entry_bytes = unsafe {
            core::slice::from_raw_parts(&entry as *const DirEntry as *const u8, size_of::<DirEntry>())
        };

        data.extend_from_slice(entry_bytes);
        self.write_inode_data(dir_inode_idx, &data)
    }

    pub fn remove_dir_entry(&self, dir_inode_idx: usize, name: &str) -> Result<u32, Errno> {
        let dir = self.read_inode(dir_inode_idx)?;
        if dir.file_type != InodeType::Directory as u8 {
            return Err(Errno::ENOTDIR);
        }

        let data = self.read_inode_data(dir_inode_idx)?;
        let count = data.len() / size_of::<DirEntry>();
        let mut new_data = Vec::with_capacity(data.len());
        let mut found_target = None;

        for i in 0..count {
            let offset = i * size_of::<DirEntry>();
            let ent: DirEntry = unsafe {
                (data.as_ptr().add(offset) as *const DirEntry).read_unaligned()
            };
            let len = ent.name.iter().position(|&b| b == 0).unwrap_or(252);
            if let Ok(entry_name) = core::str::from_utf8(&ent.name[..len]) {
                if entry_name == name && found_target.is_none() {
                    found_target = Some(ent.inode);
                    continue;
                }
            }
            let slice = unsafe {
                core::slice::from_raw_parts(&ent as *const DirEntry as *const u8, size_of::<DirEntry>())
            };
            new_data.extend_from_slice(slice);
        }

        match found_target {
            Some(target_ino) => {
                self.write_inode_data(dir_inode_idx, &new_data)?;
                Ok(target_ino)
            }
            None => Err(Errno::ENOENT),
        }
    }

    pub fn read_dir(&self, dir_inode: usize) -> Result<Vec<(u32, String, InodeType)>, Errno> {
        let dir = self.read_inode(dir_inode)?;
        if dir.file_type != InodeType::Directory as u8 {
            return Err(Errno::ENOTDIR);
        }

        let data = self.read_inode_data(dir_inode)?;
        let mut entries = Vec::new();
        let count = data.len() / size_of::<DirEntry>();

        for i in 0..count {
            let offset = i * size_of::<DirEntry>();
            let ent: DirEntry = unsafe {
                (data.as_ptr().add(offset) as *const DirEntry).read_unaligned()
            };
            let len = ent.name.iter().position(|&b| b == 0).unwrap_or(252);
            if let Ok(name_str) = core::str::from_utf8(&ent.name[..len]) {
                let target = self.read_inode(ent.inode as usize)?;
                let itype = match target.file_type {
                    1 => InodeType::Regular,
                    2 => InodeType::Directory,
                    3 => InodeType::CharDev,
                    4 => InodeType::BlockDev,
                    5 => InodeType::Fifo,
                    6 => InodeType::Symlink,
                    7 => InodeType::Socket,
                    _ => InodeType::Unused,
                };
                entries.push((ent.inode, String::from(name_str), itype));
            }
        }
        Ok(entries)
    }

    pub fn resolve_path(&self, cwd_inode: usize, path: &str) -> Result<usize, Errno> {
        self.resolve_path_inner(cwd_inode, path, 0)
    }

    fn resolve_path_inner(&self, cwd_inode: usize, path: &str, depth: usize) -> Result<usize, Errno> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(Errno::ELOOP);
        }

        let clean = path.trim();
        if clean.is_empty() {
            return Ok(cwd_inode);
        }

        let mut current = if clean.starts_with('/') { 0 } else { cwd_inode };

        for seg in clean.split('/') {
            if seg.is_empty() || seg == "." {
                continue;
            }
            if seg == ".." {
                if current == 0 {
                    continue;
                }
                let entries = self.read_dir(current)?;
                let mut parent = current;
                for (ino, name, _) in entries {
                    if name == ".." {
                        parent = ino as usize;
                        break;
                    }
                }
                current = parent;
                continue;
            }

            let entries = self.read_dir(current)?;
            let mut found = None;
            for (ino, name, itype) in entries {
                if name == seg {
                    if itype == InodeType::Symlink {
                        let link_data = self.read_inode_data(ino as usize)?;
                        let target_path = core::str::from_utf8(&link_data).map_err(|_| Errno::ENOENT)?;
                        let resolved_link = self.resolve_path_inner(current, target_path, depth + 1)?;
                        found = Some(resolved_link);
                    } else {
                        found = Some(ino as usize);
                    }
                    break;
                }
            }
            match found {
                Some(next) => current = next,
                None => return Err(Errno::ENOENT),
            }
        }
        Ok(current)
    }

    pub fn append_inode_data(&self, inode_idx: usize, data: &[u8]) -> Result<(), Errno> {
        let mut current_data = self.read_inode_data(inode_idx)?;
        current_data.extend_from_slice(data);
        self.write_inode_data(inode_idx, &current_data)
    }

    pub fn write_inode_data(&self, inode_idx: usize, data: &[u8]) -> Result<(), Errno> {
        let mut inod = self.read_inode(inode_idx)?;
        if inod.file_type == InodeType::Unused as u8 {
            return Err(Errno::EINVAL);
        }

        let total_bytes = data.len();
        let mut bytes_written = 0;

        for blk_idx in 0..6 {
            if bytes_written >= total_bytes {
                break;
            }
            if inod.blocks[blk_idx] == 0 {
                inod.blocks[blk_idx] = self.alloc_block()?;
            }
            let chunk_len = (total_bytes - bytes_written).min(SECTOR_SIZE);
            let mut buf = [0u8; SECTOR_SIZE];
            buf[..chunk_len].copy_from_slice(&data[bytes_written..bytes_written + chunk_len]);
            crate::drivers::ata::write_sector(FS_DATA_START_LBA + inod.blocks[blk_idx] as u32, &buf)?;
            bytes_written += chunk_len;
        }

        if bytes_written < total_bytes {
            let mut indirect_table = [0u16; 256];
            let mut raw_table = [0u8; SECTOR_SIZE];
            let mut is_new_indirect = false;

            if inod.indirect_block == 0 {
                inod.indirect_block = self.alloc_block()?;
                is_new_indirect = true;
            }

            if !is_new_indirect {
                crate::drivers::ata::read_sector(FS_DATA_START_LBA + inod.indirect_block as u32, &mut raw_table)?;
                unsafe {
                    core::ptr::copy_nonoverlapping(raw_table.as_ptr(), indirect_table.as_mut_ptr() as *mut u8, SECTOR_SIZE);
                }
            }

            let mut table_modified = is_new_indirect;
            for entry in indirect_table.iter_mut() {
                if bytes_written >= total_bytes {
                    break;
                }
                if *entry == 0 {
                    *entry = self.alloc_block()?;
                    table_modified = true;
                }
                let chunk_len = (total_bytes - bytes_written).min(SECTOR_SIZE);
                let mut buf = [0u8; SECTOR_SIZE];
                buf[..chunk_len].copy_from_slice(&data[bytes_written..bytes_written + chunk_len]);
                crate::drivers::ata::write_sector(FS_DATA_START_LBA + (*entry) as u32, &buf)?;
                bytes_written += chunk_len;
            }

            if table_modified {
                unsafe {
                    core::ptr::copy_nonoverlapping(indirect_table.as_ptr() as *const u8, raw_table.as_mut_ptr(), SECTOR_SIZE);
                }
                crate::drivers::ata::write_sector(FS_DATA_START_LBA + inod.indirect_block as u32, &raw_table)?;
            }
        }

        inod.size = bytes_written as u32;
        self.write_inode(inode_idx, &inod)
    }

    pub fn read_inode_data(&self, inode_idx: usize) -> Result<Vec<u8>, Errno> {
        let inod = self.read_inode(inode_idx)?;
        if inod.file_type == InodeType::Unused as u8 {
            return Err(Errno::EINVAL);
        }

        let total_size = inod.size as usize;
        let mut result = Vec::with_capacity(total_size);
        let mut bytes_read = 0;

        for blk_idx in 0..6 {
            if bytes_read >= total_size {
                break;
            }
            if inod.blocks[blk_idx] == 0 {
                break;
            }
            let chunk_len = (total_size - bytes_read).min(SECTOR_SIZE);
            let mut buf = [0u8; SECTOR_SIZE];
            crate::drivers::ata::read_sector(FS_DATA_START_LBA + inod.blocks[blk_idx] as u32, &mut buf)?;
            result.extend_from_slice(&buf[..chunk_len]);
            bytes_read += chunk_len;
        }

        if bytes_read < total_size && inod.indirect_block != 0 {
            let mut raw_table = [0u8; SECTOR_SIZE];
            let mut indirect_table = [0u16; 256];
            crate::drivers::ata::read_sector(FS_DATA_START_LBA + inod.indirect_block as u32, &mut raw_table)?;
            unsafe {
                core::ptr::copy_nonoverlapping(raw_table.as_ptr(), indirect_table.as_mut_ptr() as *mut u8, SECTOR_SIZE);
            }

            for &blk in indirect_table.iter() {
                if bytes_read >= total_size || blk == 0 {
                    break;
                }
                let chunk_len = (total_size - bytes_read).min(SECTOR_SIZE);
                let mut buf = [0u8; SECTOR_SIZE];
                crate::drivers::ata::read_sector(FS_DATA_START_LBA + blk as u32, &mut buf)?;
                result.extend_from_slice(&buf[..chunk_len]);
                bytes_read += chunk_len;
            }
        }

        Ok(result)
    }

    pub fn read_page(&self, cwd_inode: usize, path: &str, page_offset: u64, buf: &mut [u8; 4096]) -> Result<usize, Errno> {
        let ino = self.resolve_path(cwd_inode, path)?;
        let inod = self.read_inode(ino)?;
        if inod.file_type != InodeType::Regular as u8 {
            return Err(Errno::EINVAL);
        }
        let data = self.read_inode_data(ino)?;
        let start = (page_offset * 4096) as usize;
        if start >= data.len() {
            return Ok(0);
        }
        let end = (start + 4096).min(data.len());
        let len = end - start;
        buf[..len].copy_from_slice(&data[start..end]);
        Ok(len)
    }

    pub fn create_file(&self, cwd_inode: usize, path: &str, uid: u32, gid: u32) -> Result<usize, Errno> {
        let (parent_inode, base_name) = self.split_path(cwd_inode, path)?;
        let final_name = self.normalize_desktop_filename(parent_inode, base_name, false);
        if self.resolve_path(parent_inode, &final_name).is_ok() {
            return Err(Errno::EEXIST);
        }
        let ino = self.alloc_inode(InodeType::Regular, 0o644, uid, gid, 0)?;
        if let Err(e) = self.add_dir_entry(parent_inode, ino as u32, &final_name) {
            let _ = self.free_inode(ino);
            return Err(e);
        }
        Ok(ino)
    }

    pub fn create_executable_file(&self, cwd_inode: usize, path: &str, perms: u16, uid: u32, gid: u32) -> Result<usize, Errno> {
        let (parent_inode, base_name) = self.split_path(cwd_inode, path)?;
        let is_exec = (perms & 0o111) != 0;
        let final_name = self.normalize_desktop_filename(parent_inode, base_name, is_exec);
        if self.resolve_path(parent_inode, &final_name).is_ok() {
            return Err(Errno::EEXIST);
        }
        let ino = self.alloc_inode(InodeType::Regular, perms, uid, gid, 0)?;
        if let Err(e) = self.add_dir_entry(parent_inode, ino as u32, &final_name) {
            let _ = self.free_inode(ino);
            return Err(e);
        }
        Ok(ino)
    }

    pub fn create_dir(&self, cwd_inode: usize, path: &str, uid: u32, gid: u32) -> Result<(), Errno> {
        let (parent_inode, base_name) = self.split_path(cwd_inode, path)?;
        if self.resolve_path(parent_inode, base_name).is_ok() {
            return Err(Errno::EEXIST);
        }
        let new_ino = self.alloc_inode(InodeType::Directory, 0o755, uid, gid, 0)?;
        if let Err(e) = self.add_dir_entry(new_ino, new_ino as u32, ".") {
            let _ = self.free_inode(new_ino);
            return Err(e);
        }
        if let Err(e) = self.add_dir_entry(new_ino, parent_inode as u32, "..") {
            let _ = self.free_inode(new_ino);
            return Err(e);
        }
        if let Err(e) = self.add_dir_entry(parent_inode, new_ino as u32, base_name) {
            let _ = self.free_inode(new_ino);
            return Err(e);
        }
        Ok(())
    }

    pub fn mknod(&self, cwd_inode: usize, path: &str, ftype: InodeType, perms: u16, rdev: u32, uid: u32, gid: u32) -> Result<usize, Errno> {
        let (parent_inode, base_name) = self.split_path(cwd_inode, path)?;
        if self.resolve_path(parent_inode, base_name).is_ok() {
            return Err(Errno::EEXIST);
        }
        let ino = self.alloc_inode(ftype, perms, uid, gid, rdev)?;
        if let Err(e) = self.add_dir_entry(parent_inode, ino as u32, base_name) {
            let _ = self.free_inode(ino);
            return Err(e);
        }
        Ok(ino)
    }

    pub fn link(&self, cwd_inode: usize, oldpath: &str, newpath: &str) -> Result<(), Errno> {
        let target_ino = self.resolve_path(cwd_inode, oldpath)?;
        let mut inod = self.read_inode(target_ino)?;
        if inod.file_type == InodeType::Directory as u8 {
            return Err(Errno::EPERM);
        }

        let (parent_dir, file_name) = self.split_path(cwd_inode, newpath)?;
        let is_exec = (inod.permissions & 0o111) != 0;
        let final_name = self.normalize_desktop_filename(parent_dir, file_name, is_exec);
        if self.resolve_path(parent_dir, &final_name).is_ok() {
            return Err(Errno::EEXIST);
        }

        self.add_dir_entry(parent_dir, target_ino as u32, &final_name)?;
        inod.nlink += 1;
        self.write_inode(target_ino, &inod)
    }

    pub fn symlink(&self, cwd_inode: usize, target: &str, linkpath: &str, uid: u32, gid: u32) -> Result<(), Errno> {
        let (parent_dir, file_name) = self.split_path(cwd_inode, linkpath)?;
        let final_name = self.normalize_desktop_filename(parent_dir, file_name, false);
        if self.resolve_path(parent_dir, &final_name).is_ok() {
            return Err(Errno::EEXIST);
        }

        let ino = self.alloc_inode(InodeType::Symlink, 0o777, uid, gid, 0)?;
        if let Err(e) = self.add_dir_entry(parent_dir, ino as u32, &final_name) {
            let _ = self.free_inode(ino);
            return Err(e);
        }
        if let Err(e) = self.write_inode_data(ino, target.as_bytes()) {
            let _ = self.remove_dir_entry(parent_dir, &final_name);
            let _ = self.free_inode(ino);
            return Err(e);
        }
        Ok(())
    }

    pub fn unlink(&self, cwd_inode: usize, path: &str) -> Result<(), Errno> {
        let (parent_inode, base_name) = self.split_path(cwd_inode, path)?;
        let target_ino = self.remove_dir_entry(parent_inode, base_name)? as usize;
        let mut inod = self.read_inode(target_ino)?;

        if inod.nlink > 0 {
            inod.nlink -= 1;
        }

        if inod.nlink == 0 {
            for blk in inod.blocks.iter() {
                if *blk != 0 {
                    let _ = self.free_block(*blk);
                }
            }
            if inod.indirect_block != 0 {
                let _ = self.free_block(inod.indirect_block);
            }
            inod.file_type = InodeType::Unused as u8;
            inod.size = 0;
            inod.blocks = [0; 6];
            inod.indirect_block = 0;
        }

        self.write_inode(target_ino, &inod)
    }

    pub fn sync_desktop(&self) -> Result<Vec<DesktopShortcut>, Errno> {
        let desktop_ino = self.resolve_path(0, "/home/Desktop")?;
        let entries = self.read_dir(desktop_ino)?;
        let mut shortcuts = Vec::new();

        for (ino, name, itype) in entries {
            if name == "." || name == ".." {
                continue;
            }
            if itype == InodeType::Regular || itype == InodeType::Symlink {
                let inod = self.read_inode(ino as usize)?;
                let is_exec = (inod.permissions & 0o111) != 0 || name.ends_with(".rke");
                let final_name = if !name.ends_with(".ks") && !name.ends_with(".rke") {
                    let mut new_name = name.clone();
                    if is_exec {
                        new_name.push_str(".rke");
                    } else {
                        new_name.push_str(".ks");
                    }
                    let _ = self.remove_dir_entry(desktop_ino, &name);
                    let _ = self.add_dir_entry(desktop_ino, ino, &new_name);
                    new_name
                } else {
                    name
                };

                let mut path = String::from("/home/Desktop/");
                path.push_str(&final_name);
                shortcuts.push(DesktopShortcut {
                    name: final_name,
                    inode: ino,
                    is_executable: is_exec,
                    path,
                });
            }
        }

        Ok(shortcuts)
    }

    pub fn get_desktop_shortcuts(&self) -> Result<Vec<DesktopShortcut>, Errno> {
        self.sync_desktop()
    }
}

pub static VFS: Lazy<Mutex<Vfs>> = Lazy::new(|| Mutex::new(Vfs));