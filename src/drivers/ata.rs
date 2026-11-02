use crate::arch::x86_64::io::{inb, inw, outb, outw};
use crate::kernel::error::Errno;

fn ata_delay() {
    for _ in 0..5 {
        unsafe { let _ = inb(0x3F6); }
    }
}

unsafe fn wait_ready() -> Result<(), Errno> {
    ata_delay();
    for _ in 0..100_000 {
        let s = inb(0x1F7);
        if s == 0xFF {
            return Err(Errno::EIO);
        }
        if (s & 0x80) == 0 {
            if (s & 0x01) != 0 || (s & 0x20) != 0 {
                return Err(Errno::EIO);
            }
            if (s & 0x08) != 0 || (s & 0x40) != 0 {
                return Ok(());
            }
        }
    }
    Err(Errno::EIO)
}

pub fn read_sector(lba: u32, buf: &mut [u8; 512]) -> Result<(), Errno> {
    unsafe {
        outb(0x3F6, 0x02);
        ata_delay();

        outb(0x1F6, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        outb(0x1F2, 1);
        outb(0x1F3, lba as u8);
        outb(0x1F4, (lba >> 8) as u8);
        outb(0x1F5, (lba >> 16) as u8);
        outb(0x1F7, 0x20);

        wait_ready()?;

        for i in 0..256 {
            let w = inw(0x1F0);
            buf[i * 2] = w as u8;
            buf[i * 2 + 1] = (w >> 8) as u8;
        }
        Ok(())
    }
}

pub fn write_sector(lba: u32, buf: &[u8; 512]) -> Result<(), Errno> {
    unsafe {
        outb(0x3F6, 0x02);
        ata_delay();

        outb(0x1F6, 0xE0 | (((lba >> 24) & 0x0F) as u8));
        outb(0x1F2, 1);
        outb(0x1F3, lba as u8);
        outb(0x1F4, (lba >> 8) as u8);
        outb(0x1F5, (lba >> 16) as u8);
        outb(0x1F7, 0x30);

        wait_ready()?;

        for i in 0..256 {
            let w = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
            outw(0x1F0, w);
        }

        outb(0x1F7, 0xE7);
        wait_ready()
    }
}