use spin::{Lazy, Mutex};
use x86_64::instructions::port::Port;
use crate::println;

pub const RTL8139_VENDOR_ID: u16 = 0x10EC;
pub const RTL8139_DEVICE_ID: u16 = 0x8139;

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EFAULT: i32 = 14;
pub const ENODEV: i32 = 19;
pub const EINVAL: i32 = 22;
pub const ENOTSOCK: i32 = 88;
pub const EMSGSIZE: i32 = 90;
pub const EPROTOTYPE: i32 = 91;
pub const ENOPROTOOPT: i32 = 92;
pub const EPROTONOSUPPORT: i32 = 93;
pub const EAFNOSUPPORT: i32 = 97;
pub const EADDRINUSE: i32 = 98;
pub const EADDRNOTAVAIL: i32 = 99;
pub const ENETDOWN: i32 = 100;
pub const ENETUNREACH: i32 = 101;
pub const ECONNABORTED: i32 = 103;
pub const ECONNRESET: i32 = 104;
pub const ENOBUFS: i32 = 105;
pub const EISCONN: i32 = 106;
pub const ENOTCONN: i32 = 107;
pub const ETIMEDOUT: i32 = 110;
pub const ECONNREFUSED: i32 = 111;

pub const AF_UNSPEC: i32 = 0;
pub const AF_UNIX: i32 = 1;
pub const AF_INET: i32 = 2;
pub const AF_INET6: i32 = 10;

pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;
pub const SOCK_RAW: i32 = 3;

pub const IPPROTO_IP: i32 = 0;
pub const IPPROTO_ICMP: i32 = 1;
pub const IPPROTO_TCP: i32 = 6;
pub const IPPROTO_UDP: i32 = 17;
pub const IPPROTO_RAW: i32 = 255;
pub const IPPROTO_TLS: i32 = 256;

pub const DEFAULT_HOST_IP: [u8; 4] = [10, 0, 2, 15];
pub const DEFAULT_NETMASK: [u8; 4] = [255, 255, 255, 0];
pub const DEFAULT_BROADCAST: [u8; 4] = [10, 0, 2, 255];
pub const DEFAULT_GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
pub const DEFAULT_DNS_IP: [u8; 4] = [10, 0, 2, 3];
pub const DEFAULT_ROUTER_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

#[repr(C, align(4096))]
pub struct DmaRxBuffer {
    pub buffer: [u8; 12288],
}

#[repr(C, align(4096))]
pub struct DmaTxBuffers {
    pub buffer: [[u8; 2048]; 4],
}

pub static mut RX_DMA_BUFFER: DmaRxBuffer = DmaRxBuffer {
    buffer: [0; 12288],
};

pub static mut TX_DMA_BUFFERS: DmaTxBuffers = DmaTxBuffers {
    buffer: [[0; 2048]; 4],
};

#[derive(Clone, Copy, Debug)]
pub struct InterfaceConfig {
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub broadcast: [u8; 4],
    pub mac: [u8; 6],
    pub io_base: u16,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct in_addr {
    pub s_addr: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct sockaddr {
    pub sa_family: u16,
    pub sa_data: [u8; 14],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
pub struct sockaddr_in {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: in_addr,
    pub sin_zero: [u8; 8],
}

#[derive(Copy, Clone, Debug)]
pub struct TlsSession {
    pub active: bool,
    pub handshaked: bool,
    pub master_secret: [u8; 48],
    pub client_random: [u8; 32],
    pub server_random: [u8; 32],
    pub sequence_number: u64,
}

impl Default for TlsSession {
    fn default() -> Self {
        TlsSession {
            active: false,
            handshaked: false,
            master_secret: [0; 48],
            client_random: [0; 32],
            server_random: [0; 32],
            sequence_number: 0,
        }
    }
}

pub const fn htons(v: u16) -> u16 {
    v.to_be()
}

pub const fn ntohs(v: u16) -> u16 {
    u16::from_be(v)
}

pub const fn htonl(v: u32) -> u32 {
    v.to_be()
}

pub const fn ntohl(v: u32) -> u32 {
    u32::from_be(v)
}

pub fn virt_to_phys(vaddr: usize) -> u32 {
    let mapper_lock = crate::kernel::MAPPER.lock();
    if let Some(mapper) = mapper_lock.as_ref() {
        use x86_64::structures::paging::Translate;
        if let Some(phys) = mapper.translate_addr(x86_64::VirtAddr::new(vaddr as u64)) {
            let p = phys.as_u64() as u32;
            if p != 0 && (p & 0xFFF) == 0 {
                return p;
            }
        }
    }
    0x0080_0000
}

pub struct Rtl8139 {
    pub io_base: u16,
    pub mac_addr: [u8; 6],
    pub rx_phys_addr: u32,
    pub tx_phys_addrs: [u32; 4],
    pub rx_offset: usize,
    pub tx_cur: usize,
}

impl Rtl8139 {
    pub fn new(io_base: u16) -> Self {
        let rx_ptr = unsafe { core::ptr::addr_of!(RX_DMA_BUFFER.buffer) as usize };
        let rx_phys = virt_to_phys(rx_ptr);
        let mut tx_phys = [0u32; 4];
        for i in 0..4 {
            let tx_ptr = unsafe { core::ptr::addr_of!(TX_DMA_BUFFERS.buffer[i]) as usize };
            tx_phys[i] = virt_to_phys(tx_ptr);
        }

        Rtl8139 {
            io_base,
            mac_addr: [0; 6],
            rx_phys_addr: rx_phys,
            tx_phys_addrs: tx_phys,
            rx_offset: 0,
            tx_cur: 0,
        }
    }

    pub fn init(&mut self) {
        println!("[net/eth0]: Initializing RTL8139 PCI controller...");
        println!(
            "[net/eth0]: DMA RX buffer allocated at phys {:#010x} (4KB aligned)",
            self.rx_phys_addr
        );
        unsafe {
            let mut conf1 = Port::<u8>::new(self.io_base + 0x52);
            conf1.write(0x00);

            let mut cr = Port::<u8>::new(self.io_base + 0x37);
            cr.write(0x10);
            while (cr.read() & 0x10) != 0 {}

            let mut rbstart = Port::<u32>::new(self.io_base + 0x30);
            rbstart.write(self.rx_phys_addr);

            let mut imr = Port::<u16>::new(self.io_base + 0x3C);
            imr.write(0x0005);

            let mut rcr = Port::<u32>::new(self.io_base + 0x44);
            rcr.write(0x0000000F);

            cr.write(0x0C);

            for i in 0..6 {
                let mut mac_port = Port::<u8>::new(self.io_base + i as u16);
                self.mac_addr[i] = mac_port.read();
            }
        }
    }

    pub fn send_packet(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.is_empty() {
            return Err("Packet length cannot be zero");
        }
        if data.len() > 1792 {
            return Err("Packet length exceeds maximum MTU buffer");
        }

        let cur = self.tx_cur;
        let tx_status_addr = self.io_base + 0x10 + (cur * 4) as u16;
        let tx_port_addr = self.io_base + 0x20 + (cur * 4) as u16;

        unsafe {
            let mut tsd = Port::<u32>::new(tx_status_addr);
            let status = tsd.read();
            if (status & (1 << 13)) == 0 && (status & 0x1FFF) != 0 {
                return Err("TX descriptor queue ring is full");
            }

            let dst_tx = core::ptr::addr_of_mut!(TX_DMA_BUFFERS.buffer[cur]) as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst_tx, data.len());

            let mut tsad = Port::<u32>::new(tx_port_addr);
            tsad.write(self.tx_phys_addrs[cur]);
            let send_len = if data.len() < 60 { 60 } else { data.len() };
            tsd.write(send_len as u32 & 0x1FFF);
        }

        self.tx_cur = (self.tx_cur + 1) % 4;
        Ok(())
    }

    pub fn poll_packet(&mut self, out_buf: &mut [u8]) -> Option<usize> {
        unsafe {
            let mut cr = Port::<u8>::new(self.io_base + 0x37);
            if (cr.read() & 0x01) != 0 {
                return None;
            }

            let mut isr = Port::<u16>::new(self.io_base + 0x3E);
            let status = isr.read();
            if (status & 0x01) != 0 {
                isr.write(0x01);
            }

            let offset = self.rx_offset;
            let rx_ptr = core::ptr::addr_of!(RX_DMA_BUFFER.buffer) as *const u8;
            let header_status = u16::from_le_bytes([
                *rx_ptr.add(offset % 8192),
                *rx_ptr.add((offset + 1) % 8192),
            ]);
            let header_length = u16::from_le_bytes([
                *rx_ptr.add((offset + 2) % 8192),
                *rx_ptr.add((offset + 3) % 8192),
            ]) as usize;

            if (header_status & 0x01) == 0 || header_length < 4 || header_length > 1792 {
                return None;
            }

            let packet_len = header_length - 4;
            let copy_len = out_buf.len().min(packet_len);

            for i in 0..copy_len {
                out_buf[i] = *rx_ptr.add((offset + 4 + i) % 8192);
            }

            self.rx_offset = (offset + header_length + 4 + 3) & !3;
            self.rx_offset %= 8192;
            let mut capr = Port::<u16>::new(self.io_base + 0x38);
            capr.write(((self.rx_offset + 8192 - 16) % 8192) as u16);

            Some(copy_len)
        }
    }
}

pub static NIC: Lazy<Mutex<Option<Rtl8139>>> = Lazy::new(|| Mutex::new(None));

pub fn get_interface_config() -> Option<InterfaceConfig> {
    let nic_guard = NIC.lock();
    nic_guard.as_ref().map(|nic| InterfaceConfig {
        ip: DEFAULT_HOST_IP,
        netmask: DEFAULT_NETMASK,
        broadcast: DEFAULT_BROADCAST,
        mac: nic.mac_addr,
        io_base: nic.io_base,
    })
}

pub fn init_pci_network() -> Result<(), i32> {
    let mut config_addr = Port::<u32>::new(0xCF8);
    let mut config_data = Port::<u32>::new(0xCFC);

    for bus in 0..=8 {
        for slot in 0..32 {
            let addr = 0x8000_0000 | (bus << 16) | (slot << 11);
            unsafe {
                config_addr.write(addr);
                let id = config_data.read();
                let vendor = (id & 0xFFFF) as u16;
                let device = (id >> 16) as u16;

                if vendor == RTL8139_VENDOR_ID && device == RTL8139_DEVICE_ID {
                    config_addr.write(addr | 0x10);
                    let bar0 = config_data.read();
                    let io_base = (bar0 & !0x3) as u16;

                    config_addr.write(addr | 0x04);
                    let command = config_data.read();
                    config_data.write(command | 0x05);

                    let mut dev = Rtl8139::new(io_base);
                    dev.init();
                    let mac = dev.mac_addr;
                    *NIC.lock() = Some(dev);

                    println!(
                        "[net/eth0]: Link carrier detected: RTL8139 active (HWaddr {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                    );
                    return Ok(());
                }
            }
        }
    }
    println!("[net/eth0]: Network interface device not found on PCI bus");
    Err(-ENODEV)
}

pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in s.split('.') {
        if count >= 4 { return None; }
        if let Ok(val) = part.parse::<u8>() {
            octets[count] = val;
            count += 1;
        } else {
            return None;
        }
    }
    if count == 4 { Some(octets) } else { None }
}

pub fn inet_pton_af_inet(src: &str) -> Result<in_addr, i32> {
    match parse_ipv4(src) {
        Some(oct) => Ok(in_addr {
            s_addr: u32::from_ne_bytes(oct),
        }),
        None => Err(-EINVAL),
    }
}

pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        let word = ((data[i] as u32) << 8) | (data[i + 1] as u32);
        sum = sum.wrapping_add(word);
        i += 2;
    }
    if i < data.len() {
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

pub fn ping(target_ip: [u8; 4]) -> Result<u64, &'static str> {
    if target_ip == [0, 0, 0, 0] {
        return Err("Invalid destination IP address");
    }

    let mut nic_lock = NIC.lock();
    let nic = nic_lock.as_mut().ok_or("Network interface is down")?;

    let mut frame = [0u8; 1514];
    let icmp_len = 32;
    let total_ip_len = 20 + icmp_len;
    let total_frame_len = 14 + total_ip_len;

    frame[0..6].copy_from_slice(&DEFAULT_ROUTER_MAC);
    frame[6..12].copy_from_slice(&nic.mac_addr);
    frame[12] = 0x08;
    frame[13] = 0x00;

    frame[14] = 0x45;
    frame[15] = 0x00;
    frame[16] = (total_ip_len >> 8) as u8;
    frame[17] = (total_ip_len & 0xFF) as u8;
    frame[18] = 0x54;
    frame[19] = 0x32;
    frame[20] = 0x40;
    frame[21] = 0x00;
    frame[22] = 64;
    frame[23] = IPPROTO_ICMP as u8;
    frame[24] = 0x00;
    frame[25] = 0x00;
    frame[26..30].copy_from_slice(&DEFAULT_HOST_IP);
    frame[30..34].copy_from_slice(&target_ip);

    let ip_cs = checksum(&frame[14..34]);
    frame[24] = (ip_cs >> 8) as u8;
    frame[25] = (ip_cs & 0xFF) as u8;

    let off = 34;
    frame[off] = 8;
    frame[off + 1] = 0;
    frame[off + 2] = 0;
    frame[off + 3] = 0;
    frame[off + 4] = 0x13;
    frame[off + 5] = 0x37;
    frame[off + 6] = 0x00;
    frame[off + 7] = 0x01;

    for i in 8..icmp_len {
        frame[off + i] = (i & 0xFF) as u8;
    }

    let icmp_cs = checksum(&frame[off..off + icmp_len]);
    frame[off + 2] = (icmp_cs >> 8) as u8;
    frame[off + 3] = (icmp_cs & 0xFF) as u8;

    let _ = nic.send_packet(&frame[..total_frame_len]);
    drop(nic_lock);

    let mut rx_buf = [0u8; 1514];
    for t in 0..5_000_000 {
        x86_64::instructions::nop();
        let mut nic_lock = NIC.lock();
        if let Some(nic) = nic_lock.as_mut() {
            if let Some(len) = nic.poll_packet(&mut rx_buf) {
                if len >= 42 && rx_buf[12] == 0x08 && rx_buf[13] == 0x00 && rx_buf[23] == IPPROTO_ICMP as u8 {
                    if rx_buf[26..30] == target_ip {
                        let ip_hdr_len = (rx_buf[14] & 0x0F) as usize * 4;
                        let icmp_off = 14 + ip_hdr_len;
                        if len >= icmp_off + 8 {
                            let icmp_type = rx_buf[icmp_off];
                            let icmp_code = rx_buf[icmp_off + 1];
                            let icmp_id = ((rx_buf[icmp_off + 4] as u16) << 8) | (rx_buf[icmp_off + 5] as u16);
                            if icmp_type == 0 && icmp_code == 0 && icmp_id == 0x1337 {
                                let rtt = (t / 100_000).max(1) as u64;
                                return Ok(rtt);
                            }
                        }
                    }
                }
            }
        }
    }

    Err("Request timeout")
}

pub fn send_udp_packet(dst_ip: [u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> Result<(), &'static str> {
    let mut nic_lock = NIC.lock();
    let nic = nic_lock.as_mut().ok_or("NIC device not initialized")?;

    let mut frame = [0u8; 1514];
    let total_udp_len = 8 + payload.len();
    let total_ip_len = 20 + total_udp_len;
    let total_frame_len = 14 + total_ip_len;

    frame[0..6].copy_from_slice(&DEFAULT_ROUTER_MAC);
    frame[6..12].copy_from_slice(&nic.mac_addr);
    frame[12] = 0x08;
    frame[13] = 0x00;

    frame[14] = 0x45;
    frame[15] = 0x00;
    frame[16] = (total_ip_len >> 8) as u8;
    frame[17] = (total_ip_len & 0xFF) as u8;
    frame[18] = 0x13;
    frame[19] = 0x37;
    frame[20] = 0x40;
    frame[21] = 0x00;
    frame[22] = 64;
    frame[23] = IPPROTO_UDP as u8;
    frame[24] = 0x00;
    frame[25] = 0x00;
    frame[26..30].copy_from_slice(&DEFAULT_HOST_IP);
    frame[30..34].copy_from_slice(&dst_ip);

    let ip_cs = checksum(&frame[14..34]);
    frame[24] = (ip_cs >> 8) as u8;
    frame[25] = (ip_cs & 0xFF) as u8;

    frame[34] = (src_port >> 8) as u8;
    frame[35] = (src_port & 0xFF) as u8;
    frame[36] = (dst_port >> 8) as u8;
    frame[37] = (dst_port & 0xFF) as u8;
    frame[38] = (total_udp_len >> 8) as u8;
    frame[39] = (total_udp_len & 0xFF) as u8;
    frame[40] = 0x00;
    frame[41] = 0x00;

    frame[42..42 + payload.len()].copy_from_slice(payload);

    nic.send_packet(&frame[..total_frame_len])
}

pub fn dns_resolve(domain: &str) -> Result<[u8; 4], &'static str> {
    if domain.is_empty() || domain.len() > 253 {
        return Err("Invalid domain length for resolution");
    }

    if let Some(ip) = parse_ipv4(domain) {
        return Ok(ip);
    }

    println!("[net/dns]: Resolving A-record query for host {}", domain);

    let mut query = [0u8; 512];
    let tx_id = 0xA77C;
    query[0] = (tx_id >> 8) as u8;
    query[1] = (tx_id & 0xFF) as u8;
    query[2] = 0x01;
    query[3] = 0x00;
    query[4] = 0x00;
    query[5] = 0x01;
    query[6] = 0x00;
    query[7] = 0x00;
    query[8] = 0x00;
    query[9] = 0x00;
    query[10] = 0x00;
    query[11] = 0x00;

    let mut q_idx = 12;
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err("Invalid domain label format");
        }
        query[q_idx] = label.len() as u8;
        q_idx += 1;
        for b in label.bytes() {
            query[q_idx] = b;
            q_idx += 1;
        }
    }
    query[q_idx] = 0x00;
    q_idx += 1;
    query[q_idx] = 0x00;
    query[q_idx + 1] = 0x01;
    query[q_idx + 2] = 0x00;
    query[q_idx + 3] = 0x01;
    q_idx += 4;

    let src_port = 53053;
    send_udp_packet(DEFAULT_DNS_IP, src_port, 53, &query[..q_idx])?;

    let mut rx_buf = [0u8; 1514];
    for _ in 0..2_000_000 {
        x86_64::instructions::nop();
        let mut nic_lock = NIC.lock();
        if let Some(nic) = nic_lock.as_mut() {
            if let Some(len) = nic.poll_packet(&mut rx_buf) {
                if len >= 42 && rx_buf[12] == 0x08 && rx_buf[13] == 0x00 && rx_buf[23] == 17 {
                    let d_port = ((rx_buf[36] as u16) << 8) | (rx_buf[37] as u16);
                    if d_port == src_port {
                        let dns_payload = &rx_buf[42..len];
                        if dns_payload.len() >= 12 {
                            let resp_id = ((dns_payload[0] as u16) << 8) | (dns_payload[1] as u16);
                            let an_count = ((dns_payload[6] as u16) << 8) | (dns_payload[7] as u16);
                            if resp_id == tx_id && an_count > 0 {
                                let mut ptr = 12;
                                while ptr < dns_payload.len() && dns_payload[ptr] != 0 {
                                    if (dns_payload[ptr] & 0xC0) == 0xC0 {
                                        ptr += 2;
                                        break;
                                    }
                                    ptr += 1 + dns_payload[ptr] as usize;
                                }
                                if ptr < dns_payload.len() && dns_payload[ptr] == 0 {
                                    ptr += 1;
                                }
                                ptr += 4;

                                for _ in 0..an_count {
                                    if ptr >= dns_payload.len() { break; }
                                    if (dns_payload[ptr] & 0xC0) == 0xC0 {
                                        ptr += 2;
                                    } else {
                                        while ptr < dns_payload.len() && dns_payload[ptr] != 0 {
                                            ptr += 1 + dns_payload[ptr] as usize;
                                        }
                                        ptr += 1;
                                    }
                                    if ptr + 10 > dns_payload.len() { break; }
                                    let r_type = ((dns_payload[ptr] as u16) << 8) | (dns_payload[ptr + 1] as u16);
                                    let rd_len = ((dns_payload[ptr + 8] as usize) << 8) | (dns_payload[ptr + 9] as usize);
                                    ptr += 10;
                                    if r_type == 1 && rd_len == 4 && ptr + 4 <= dns_payload.len() {
                                        let ip = [dns_payload[ptr], dns_payload[ptr + 1], dns_payload[ptr + 2], dns_payload[ptr + 3]];
                                        println!("[net/dns]: Resolved {} -> {}.{}.{}.{}", domain, ip[0], ip[1], ip[2], ip[3]);
                                        return Ok(ip);
                                    }
                                    ptr += rd_len;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err("DNS query request timed out")
}

pub fn send_tcp_packet(
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    options: &[u8],
    payload: &[u8],
) -> Result<(), &'static str> {
    let mut nic_lock = NIC.lock();
    let nic = nic_lock.as_mut().ok_or("NIC device offline")?;

    let mut frame = [0u8; 1514];
    let tcp_hdr_len = 20 + options.len();
    let total_tcp_len = tcp_hdr_len + payload.len();
    let total_ip_len = 20 + total_tcp_len;
    let total_frame_len = 14 + total_ip_len;

    frame[0..6].copy_from_slice(&DEFAULT_ROUTER_MAC);
    frame[6..12].copy_from_slice(&nic.mac_addr);
    frame[12] = 0x08;
    frame[13] = 0x00;

    frame[14] = 0x45;
    frame[15] = 0x00;
    frame[16] = (total_ip_len >> 8) as u8;
    frame[17] = (total_ip_len & 0xFF) as u8;
    frame[18] = 0x42;
    frame[19] = 0x42;
    frame[20] = 0x40;
    frame[21] = 0x00;
    frame[22] = 64;
    frame[23] = IPPROTO_TCP as u8;
    frame[24] = 0x00;
    frame[25] = 0x00;
    frame[26..30].copy_from_slice(&DEFAULT_HOST_IP);
    frame[30..34].copy_from_slice(&dst_ip);

    let ip_cs = checksum(&frame[14..34]);
    frame[24] = (ip_cs >> 8) as u8;
    frame[25] = (ip_cs & 0xFF) as u8;

    let off = 34;
    frame[off] = (src_port >> 8) as u8;
    frame[off + 1] = (src_port & 0xFF) as u8;
    frame[off + 2] = (dst_port >> 8) as u8;
    frame[off + 3] = (dst_port & 0xFF) as u8;
    frame[off + 4..off + 8].copy_from_slice(&seq.to_be_bytes());
    frame[off + 8..off + 12].copy_from_slice(&ack.to_be_bytes());
    
    frame[off + 12] = ((5 + (options.len() / 4)) as u8) << 4;
    frame[off + 13] = flags;
    frame[off + 14] = 0xFA;
    frame[off + 15] = 0xF0;
    frame[off + 16] = 0x00;
    frame[off + 17] = 0x00;
    frame[off + 18] = 0x00;
    frame[off + 19] = 0x00;

    if !options.is_empty() {
        frame[off + 20..off + 20 + options.len()].copy_from_slice(options);
    }
    
    if !payload.is_empty() {
        frame[off + 20 + options.len()..off + 20 + options.len() + payload.len()].copy_from_slice(payload);
    }

    let mut pseudo = [0u8; 1514];
    pseudo[0..4].copy_from_slice(&DEFAULT_HOST_IP);
    pseudo[4..8].copy_from_slice(&dst_ip);
    pseudo[8] = 0;
    pseudo[9] = IPPROTO_TCP as u8;
    pseudo[10] = (total_tcp_len >> 8) as u8;
    pseudo[11] = (total_tcp_len & 0xFF) as u8;
    pseudo[12..12 + total_tcp_len].copy_from_slice(&frame[off..off + total_tcp_len]);

    let tcp_cs = checksum(&pseudo[..12 + total_tcp_len]);
    frame[off + 16] = (tcp_cs >> 8) as u8;
    frame[off + 17] = (tcp_cs & 0xFF) as u8;

    nic.send_packet(&frame[..total_frame_len])
}

pub fn curl_get(url: &str, out_body: &mut [u8]) -> Result<usize, &'static str> {
    let clean_url = if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        url
    };

    let (host_port, path) = match clean_url.find('/') {
        Some(pos) => (&clean_url[..pos], &clean_url[pos..]),
        None => (clean_url, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(pos) => {
            let h = &host_port[..pos];
            let p = host_port[pos + 1..].parse::<u16>().map_err(|_| "Invalid port syntax")?;
            (h, p)
        }
        None => (host_port, 80),
    };

    let target_ip = dns_resolve(host)?;
    println!("[net/http]: Connecting via TCP 3-Way Handshake to {}:{}{}...", host, port, path);

    let src_port = 49152;
    let mut my_seq: u32 = 0x1000;
    let mut their_seq: u32 = 0;

    let syn_options = [0x02, 0x04, 0x05, 0xB4, 0x03, 0x03, 0x07, 0x00];
    send_tcp_packet(target_ip, src_port, port, my_seq, 0, 0x02, &syn_options, &[])?;
    my_seq = my_seq.wrapping_add(1);

    let mut rx_buf = [0u8; 1514];
    let mut synack_received = false;

    for _ in 0..2_500_000 {
        x86_64::instructions::nop();
        let mut nic_lock = NIC.lock();
        if let Some(nic) = nic_lock.as_mut() {
            if let Some(len) = nic.poll_packet(&mut rx_buf) {
                if len >= 54 && rx_buf[12] == 0x08 && rx_buf[13] == 0x00 && rx_buf[23] == 6 {
                    let d_port = ((rx_buf[36] as u16) << 8) | (rx_buf[37] as u16);
                    let s_port = ((rx_buf[34] as u16) << 8) | (rx_buf[35] as u16);
                    if d_port == src_port && s_port == port {
                        let flags = rx_buf[47];
                        if (flags & 0x12) == 0x12 {
                            let seq_in = u32::from_be_bytes([rx_buf[38], rx_buf[39], rx_buf[40], rx_buf[41]]);
                            their_seq = seq_in.wrapping_add(1);
                            synack_received = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    if !synack_received {
        return Err("Connection failed: TCP SYN-ACK timeout");
    }

    send_tcp_packet(target_ip, src_port, port, my_seq, their_seq, 0x10, &[], &[])?;

    let mut req_buf = [0u8; 512];
    let mut r_idx = 0;
    let get_prefix = b"GET ";
    req_buf[r_idx..r_idx + get_prefix.len()].copy_from_slice(get_prefix);
    r_idx += get_prefix.len();
    req_buf[r_idx..r_idx + path.len()].copy_from_slice(path.as_bytes());
    r_idx += path.len();
    let get_mid = b" HTTP/1.1\r\nHost: ";
    req_buf[r_idx..r_idx + get_mid.len()].copy_from_slice(get_mid);
    r_idx += get_mid.len();
    req_buf[r_idx..r_idx + host.len()].copy_from_slice(host.as_bytes());
    r_idx += host.len();
    let get_end = b"\r\nConnection: close\r\nUser-Agent: redKiteOS/2.0-POSIX-Compliant\r\n\r\n";
    req_buf[r_idx..r_idx + get_end.len()].copy_from_slice(get_end);
    r_idx += get_end.len();

    send_tcp_packet(target_ip, src_port, port, my_seq, their_seq, 0x18, &[], &req_buf[..r_idx])?;
    my_seq = my_seq.wrapping_add(r_idx as u32);

    let mut raw_response = [0u8; 8192];
    let mut raw_len = 0;

    for _ in 0..8_000_000 {
        x86_64::instructions::nop();
        let mut nic_lock = NIC.lock();
        if let Some(nic) = nic_lock.as_mut() {
            if let Some(len) = nic.poll_packet(&mut rx_buf) {
                if len >= 54 && rx_buf[12] == 0x08 && rx_buf[13] == 0x00 && rx_buf[23] == 6 {
                    let d_port = ((rx_buf[36] as u16) << 8) | (rx_buf[37] as u16);
                    let s_port = ((rx_buf[34] as u16) << 8) | (rx_buf[35] as u16);
                    if d_port == src_port && s_port == port {
                        let flags = rx_buf[47];
                        let ip_hdr_len = (rx_buf[14] & 0x0F) as usize * 4;
                        let tcp_hdr_len = ((rx_buf[14 + ip_hdr_len + 12] >> 4) as usize) * 4;
                        let payload_offset = 14 + ip_hdr_len + tcp_hdr_len;

                        if len > payload_offset {
                            let p_len = len - payload_offset;
                            if raw_len + p_len <= raw_response.len() {
                                raw_response[raw_len..raw_len + p_len].copy_from_slice(&rx_buf[payload_offset..len]);
                                raw_len += p_len;
                            }
                            let seq_in = u32::from_be_bytes([rx_buf[38], rx_buf[39], rx_buf[40], rx_buf[41]]);
                            their_seq = seq_in.wrapping_add(p_len as u32);
                            drop(nic_lock);
                            let _ = send_tcp_packet(target_ip, src_port, port, my_seq, their_seq, 0x10, &[], &[]);
                        } else if (flags & 0x01) != 0 {
                            drop(nic_lock);
                            let _ = send_tcp_packet(target_ip, src_port, port, my_seq, their_seq.wrapping_add(1), 0x11, &[], &[]);
                            break;
                        }
                    }
                }
            }
        }
    }

    if raw_len == 0 {
        return Err("No data received from target host");
    }

    let mut body_start = raw_len;
    for i in 0..raw_len.saturating_sub(3) {
        if raw_response[i] == b'\r' && raw_response[i + 1] == b'\n' &&
           raw_response[i + 2] == b'\r' && raw_response[i + 3] == b'\n' {
            body_start = i + 4;
            break;
        }
    }

    let body_len = raw_len - body_start;
    let copy_size = out_body.len().min(body_len);
    out_body[..copy_size].copy_from_slice(&raw_response[body_start..body_start + copy_size]);

    println!("[net/http]: Response transfer complete: payload size {} bytes", copy_size);
    Ok(copy_size)
}

pub fn sys_socket(domain: i32, socket_type: i32, protocol: i32) -> Result<i32, i32> {
    if domain != AF_INET && domain != AF_UNIX {
        return Err(-EAFNOSUPPORT);
    }
    if socket_type != SOCK_STREAM && socket_type != SOCK_DGRAM && socket_type != SOCK_RAW {
        return Err(-EINVAL);
    }
    if protocol != IPPROTO_IP && protocol != IPPROTO_TCP && protocol != IPPROTO_UDP && protocol != IPPROTO_ICMP && protocol != IPPROTO_TLS {
        return Err(-EPROTONOSUPPORT);
    }
    Ok(3)
}

pub fn sys_bind(sockfd: i32, addr: *const sockaddr, addrlen: u32) -> Result<(), i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if addr.is_null() {
        return Err(-EFAULT);
    }
    if addrlen < core::mem::size_of::<sockaddr_in>() as u32 {
        return Err(-EINVAL);
    }
    Ok(())
}

pub fn sys_connect(sockfd: i32, addr: *const sockaddr, addrlen: u32) -> Result<(), i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if addr.is_null() {
        return Err(-EFAULT);
    }
    if addrlen < core::mem::size_of::<sockaddr_in>() as u32 {
        return Err(-EINVAL);
    }
    Ok(())
}

pub fn sys_listen(sockfd: i32, backlog: i32) -> Result<(), i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if backlog < 0 {
        return Err(-EINVAL);
    }
    Ok(())
}

pub fn sys_accept(sockfd: i32, addr: *mut sockaddr, addrlen: *mut u32) -> Result<i32, i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if !addr.is_null() && addrlen.is_null() {
        return Err(-EFAULT);
    }
    Ok(4)
}

pub fn sys_send(sockfd: i32, buf: *const u8, len: usize, _flags: i32) -> Result<usize, i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if buf.is_null() && len > 0 {
        return Err(-EFAULT);
    }
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    if let Some(nic) = NIC.lock().as_mut() {
        match nic.send_packet(slice) {
            Ok(()) => Ok(len),
            Err(_) => Err(-ENOBUFS),
        }
    } else {
        Err(-ENETDOWN)
    }
}

pub fn sys_recv(sockfd: i32, buf: *mut u8, len: usize, _flags: i32) -> Result<usize, i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if buf.is_null() && len > 0 {
        return Err(-EFAULT);
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    if let Some(nic) = NIC.lock().as_mut() {
        match nic.poll_packet(slice) {
            Some(n) => Ok(n),
            None => Err(-EAGAIN),
        }
    } else {
        Err(-ENETDOWN)
    }
}

pub fn sys_sendto(sockfd: i32, buf: *const u8, len: usize, flags: i32, dest_addr: *const sockaddr, addrlen: u32) -> Result<usize, i32> {
    if !dest_addr.is_null() && addrlen < core::mem::size_of::<sockaddr_in>() as u32 {
        return Err(-EINVAL);
    }
    sys_send(sockfd, buf, len, flags)
}

pub fn sys_recvfrom(sockfd: i32, buf: *mut u8, len: usize, flags: i32, src_addr: *mut sockaddr, addrlen: *mut u32) -> Result<usize, i32> {
    if !src_addr.is_null() && addrlen.is_null() {
        return Err(-EFAULT);
    }
    sys_recv(sockfd, buf, len, flags)
}

pub fn sys_shutdown(sockfd: i32, how: i32) -> Result<(), i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    if how != 0 && how != 1 && how != 2 {
        return Err(-EINVAL);
    }
    Ok(())
}

pub fn sys_close(sockfd: i32) -> Result<(), i32> {
    if sockfd < 0 {
        return Err(-EBADF);
    }
    Ok(())
}